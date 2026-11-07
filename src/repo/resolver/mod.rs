//! _Resolve with agent_: an agent resolves this checkout's conflicts in the background, in a
//! scratch worktree out of the user's way (see [`crate::resolver`]), while the tab shows its
//! plan, each step and what it is doing on a card of its own ([`panel`]). Once it is done the
//! user applies what it resolved: those files are written here and marked resolved.

mod panel;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::repo::{OpKind, Refresh, RepoTab};
use crate::resolver::agent::{self, Activity, Engine};
use crate::resolver::process::Running;
use crate::resolver::report::{Progress, Report};
use crate::resolver::workspace::{FileResult, Workspace};

/// What the card and the buttons that start a run ask for.
#[derive(Clone, Debug)]
pub(crate) enum ResolverAct {
    /// Resolve these files (every conflicted one when `None`).
    Start(Option<Vec<String>>),
    Stop,
    /// Apply the files picked, and mark them resolved.
    Apply,
    /// Picks a file to apply, or leaves it out.
    Pick(String, bool),
    /// Forget the run, its worktree removed.
    Dismiss,
    /// Folds the card to a pill, or opens it again.
    Fold(bool),
}

/// Where a run is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Making the worktree.
    Preparing,
    /// The agent works.
    Working,
    /// It ended: its files are being looked at.
    Checking,
    /// Its files are there to apply.
    Ready,
    Applying,
    Applied,
    /// It couldn't start, or ended without anything to apply.
    Failed,
}

/// One run of the agent.
struct Run {
    id: String,
    engine: Engine,
    files: Vec<String>,
    started: Instant,
    ended: Option<Instant>,
    phase: Phase,
    progress: Progress,
    workspace: Option<Arc<Workspace>>,
    agent: Option<Running>,
    /// Claude Code's MCP configuration, removed when the run ends.
    mcp_file: Option<PathBuf>,
    /// The user asked it to stop.
    stopping: bool,
    /// How each file came back, once it ended.
    results: Vec<(String, FileResult)>,
    picked: BTreeSet<String>,
    applied: usize,
    error: Option<String>,
    /// What the agent said last, and what it cost (Claude Code says).
    last_words: Option<String>,
    cost_usd: Option<f64>,
}

/// The tab's run, if one.
#[derive(Default)]
pub(crate) struct ResolverState {
    run: Option<Run>,
    folded: bool,
}

impl ResolverState {
    /// Whether a run is under way or waits for the user: another can't start.
    pub(crate) fn busy(&self) -> bool {
        self.run.is_some()
    }
}

/// How often the card's clock moves.
const CLOCK: Duration = Duration::from_secs(1);

/// Removes what a run leaves, in the background (removing a worktree runs git).
fn clean_up(run: &mut Run) {
    if let Some(agent) = run.agent.take() {
        agent.stop();
    }
    let workspace = run.workspace.take();
    let mcp_file = run.mcp_file.take();
    std::thread::spawn(move || {
        if let Some(file) = mcp_file {
            let _ = std::fs::remove_file(file);
        }
        if let Some(ws) = workspace.and_then(|ws| Arc::try_unwrap(ws).ok()) {
            let _ = ws.remove();
        }
    });
}

impl Drop for ResolverState {
    fn drop(&mut self) {
        if let Some(run) = &mut self.run {
            clean_up(run);
        }
    }
}

/// What the run needs to start the agent, made in the background.
struct Prepared {
    workspace: Workspace,
    mcp_file: PathBuf,
    command: crate::ai::run::CommandLine,
    prompt: String,
}

fn prepare(
    repo: &std::path::Path,
    files: &[String],
    engine: Engine,
    id: &str,
) -> Result<Prepared, String> {
    let exe = std::env::current_exe().map_err(|e| format!("gitcraft can't find itself: {e}"))?;
    let workspace = Workspace::prepare(repo, files)?;
    let mcp_file = std::env::temp_dir().join(format!("gitcraft-resolve-{id}.json"));
    let config = agent::claude_mcp_config(&exe, workspace.dir(), id);
    std::fs::write(&mcp_file, config.to_string())
        .map_err(|e| format!("couldn't write the agent's MCP settings: {e}"))?;
    let command = agent::command(engine, &exe, workspace.dir(), id, &mcp_file)?;
    let prompt = agent::prompt(&workspace);
    Ok(Prepared {
        workspace,
        mcp_file,
        command,
        prompt,
    })
}

/// Why an agent that ended with `code` and said `stderr` gave nothing, as the user reads it.
fn failure(engine: Engine, code: Option<i32>, stderr: &str) -> String {
    let program = match engine {
        Engine::Claude => "claude",
        Engine::Codex => "codex",
    };
    match code {
        Some(127 | 9009) => format!(
            "{program} isn't installed here (or isn't on the PATH gitcraft's commands get). \
             Install {}, or pick the other agent in Settings → AI.",
            engine.label()
        ),
        _ => {
            let said: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
            let tail = said[said.len().saturating_sub(4)..].join(" ");
            let how =
                code.map_or_else(|| "stopped".to_owned(), |c| format!("exited with code {c}"));
            if tail.is_empty() {
                format!("{} {how}", engine.label())
            } else {
                format!(
                    "{} {how}: {}",
                    engine.label(),
                    crate::resolver::report::clean(&tail, 400)
                )
            }
        }
    }
}

impl RepoTab {
    pub(crate) fn resolver_act(&mut self, act: ResolverAct, ctx: &egui::Context) {
        match act {
            ResolverAct::Start(files) => self.start_resolver(files, ctx),
            ResolverAct::Stop => {
                if let Some(run) = &mut self.resolver.run {
                    run.stopping = true;
                    if let Some(agent) = &run.agent {
                        agent.stop();
                    }
                }
            }
            ResolverAct::Apply => self.apply_resolved(ctx),
            ResolverAct::Pick(path, on) => {
                if let Some(run) = &mut self.resolver.run {
                    if on {
                        run.picked.insert(path);
                    } else {
                        run.picked.remove(&path);
                    }
                }
            }
            ResolverAct::Dismiss => {
                if let Some(mut run) = self.resolver.run.take() {
                    clean_up(&mut run);
                }
                self.resolver.folded = false;
            }
            ResolverAct::Fold(folded) => self.resolver.folded = folded,
        }
    }

    /// Whether the run `id` is this tab's.
    pub(crate) fn resolver_owns(&self, id: &str) -> bool {
        self.resolver.run.as_ref().is_some_and(|r| r.id == id)
    }

    /// A report of the agent's.
    pub(crate) fn resolver_report(&mut self, report: Report, ctx: &egui::Context) {
        if let Some(run) = &mut self.resolver.run {
            run.progress.apply(report, Instant::now());
            ctx.request_repaint();
        }
    }

    fn start_resolver(&mut self, files: Option<Vec<String>>, ctx: &egui::Context) {
        if self.resolver.busy() {
            self.resolver.folded = false;
            return;
        }
        let files = files.unwrap_or_else(|| {
            self.work
                .conflicted
                .iter()
                .map(|f| f.path.clone())
                .collect()
        });
        if files.is_empty() {
            self.status = Some(("There is no conflicted file to resolve".to_owned(), true));
            return;
        }
        let engine = Engine::from_settings(crate::settings::Settings::get(ctx).ai.tool);
        let id = crate::resolver::run_id();
        self.resolver.folded = false;
        self.resolver.run = Some(Run {
            id: id.clone(),
            engine,
            files: files.clone(),
            started: Instant::now(),
            ended: None,
            phase: Phase::Preparing,
            progress: Progress::default(),
            workspace: None,
            agent: None,
            mcp_file: None,
            stopping: false,
            results: Vec::new(),
            picked: BTreeSet::new(),
            applied: 0,
            error: None,
            last_words: None,
            cost_usd: None,
        });
        let repo = self.path.clone();
        let for_id = id.clone();
        self.then(
            ctx,
            move || prepare(&repo, &files, engine, &for_id),
            move |tab, prepared, ctx| tab.resolver_prepared(&id, prepared, ctx),
        );
    }

    fn resolver_prepared(
        &mut self,
        id: &str,
        prepared: Result<Prepared, String>,
        ctx: &egui::Context,
    ) {
        let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == id) else {
            // Dismissed meanwhile: what was made goes.
            if let Ok(p) = prepared {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_file(&p.mcp_file);
                    let _ = p.workspace.remove();
                });
            }
            return;
        };
        let p = match prepared {
            Ok(p) => p,
            Err(e) => {
                run.phase = Phase::Failed;
                run.error = Some(e);
                run.ended = Some(Instant::now());
                return;
            }
        };
        let repaint = ctx.clone();
        let started = Running::start(
            run.engine,
            &p.command,
            p.workspace.dir(),
            p.prompt,
            move || repaint.request_repaint(),
        );
        run.workspace = Some(Arc::new(p.workspace));
        run.mcp_file = Some(p.mcp_file);
        match started {
            Ok(agent) => {
                run.agent = Some(agent);
                run.phase = Phase::Working;
            }
            Err(e) => {
                run.phase = Phase::Failed;
                run.error = Some(e);
                run.ended = Some(Instant::now());
                clean_up(run);
            }
        }
    }

    /// Every frame: an agent that ended has its files looked at; while one works, the card's
    /// clock moves.
    pub(super) fn resolver_frame(&mut self, ctx: &egui::Context) {
        let Some(run) = &mut self.resolver.run else {
            return;
        };
        if matches!(run.phase, Phase::Preparing | Phase::Working) {
            ctx.request_repaint_after(CLOCK);
        }
        if run.phase != Phase::Working {
            return;
        }
        let Some(exit) = run
            .agent
            .as_ref()
            .and_then(|a| a.with_feed(|f| f.exit.clone()))
        else {
            return;
        };
        if let Some(Activity::Ended { text, cost_usd, .. }) = run.agent.as_ref().and_then(|a| {
            a.with_feed(|f| {
                f.activities
                    .iter()
                    .rev()
                    .find(|a| matches!(a, Activity::Ended { .. }))
                    .cloned()
            })
        }) {
            run.last_words = Some(text).filter(|t| !t.is_empty());
            run.cost_usd = cost_usd;
        }
        run.ended = Some(Instant::now());
        let failed = !exit.stopped && exit.code != Some(0) && run.progress.finished.is_none();
        if failed {
            run.error = Some(failure(run.engine, exit.code, &exit.stderr));
        }
        let Some(ws) = run.workspace.clone() else {
            run.phase = Phase::Failed;
            return;
        };
        run.phase = Phase::Checking;
        let (repo, id) = (self.path.clone(), run.id.clone());
        self.then(
            ctx,
            move || ws.check(&repo),
            move |tab, results, _| {
                let Some(run) = tab.resolver.run.as_mut().filter(|r| r.id == id) else {
                    return;
                };
                run.picked = results
                    .iter()
                    .filter(|(_, r)| r.applies())
                    .map(|(p, _)| p.clone())
                    .collect();
                let any = !run.picked.is_empty();
                run.results = results;
                run.phase = if any || run.error.is_none() {
                    Phase::Ready
                } else {
                    Phase::Failed
                };
            },
        );
    }

    fn apply_resolved(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the running operation to finish".to_owned(), true));
            return;
        }
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Ready)
        else {
            return;
        };
        let Some(ws) = run.workspace.clone() else {
            return;
        };
        let picked: Vec<String> = run.picked.iter().cloned().collect();
        if picked.is_empty() {
            return;
        }
        run.phase = Phase::Applying;
        let (repo, id) = (self.path.clone(), run.id.clone());
        self.then(
            ctx,
            move || ws.apply(&repo, &picked),
            move |tab, written, ctx| tab.resolved_applied(&id, written, ctx),
        );
    }

    fn resolved_applied(
        &mut self,
        id: &str,
        written: Result<Vec<String>, String>,
        ctx: &egui::Context,
    ) {
        let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        let written = match written {
            Ok(w) => w,
            Err(e) => {
                run.phase = Phase::Ready;
                self.status = Some((format!("The agent's files weren't all applied: {e}"), true));
                return;
            }
        };
        run.applied = written.len();
        run.phase = Phase::Applied;
        clean_up(run);
        if written.is_empty() {
            self.status = Some((
                "Nothing was applied: the files changed here since the agent started".to_owned(),
                true,
            ));
            return;
        }
        // The conflict editor shows the file as it was: it is closed over the new one.
        if self
            .conflict
            .as_ref()
            .is_some_and(|ed| written.contains(&ed.path))
        {
            self.queued.push(crate::repo::Action::CloseDiff);
        }
        let mut add = ["--literal-pathspecs", "add", "-A", "--"]
            .map(str::to_owned)
            .to_vec();
        add.extend(written);
        self.run_ops(
            "Mark resolved (agent)",
            vec![add],
            Refresh::Status,
            OpKind::plain(),
            ctx,
        );
    }
}

#[cfg(test)]
mod look_tests;
