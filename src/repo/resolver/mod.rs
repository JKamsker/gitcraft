//! _Resolve with agent_: an agent resolves this checkout's conflicts in the background, in a
//! scratch worktree out of the user's way (see [`crate::resolver`]), while the tab shows its
//! plan, each step and what it is doing on a card of its own ([`panel`]). Once it is done the
//! user applies what it resolved: those files are written here and marked resolved.

mod apply;
mod detail;
mod graph;
mod panel;
mod parking;
mod review;
mod review_view;

pub(in crate::repo) use graph::parked_rows;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::repo::RepoTab;
use crate::resolver::agent::{self, Activity, Engine};
use crate::resolver::park::{self, Parked};
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
    /// Shows what the agent did to the files, at the file of this index.
    Review(usize),
    /// Shows the file reviewed by conflict, or as a line diff.
    ReviewAsDiff(bool),
    CloseReview,
    /// Brings the merge put aside as this back into the worktree.
    BringBack(String),
    /// Drops the merge put aside as this, its files kept on a branch of their own.
    DropParked(String),
}

/// The agent's work under review, in the big area.
struct ReviewState {
    /// What it did to each file, in the order of the run's results; `None` while read.
    files: Option<Vec<(String, review::FileReview)>>,
    selected: usize,
    as_diff: bool,
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
    /// The files applied were written, but marking them resolved failed.
    unstaged: bool,
    error: Option<String>,
    /// What the agent said last, and what it cost (Claude Code says).
    last_words: Option<String>,
    cost_usd: Option<f64>,
    /// Its files under review, while they are.
    review: Option<ReviewState>,
    /// The merge it resolves, put aside while it does (`None`: resolved in place, or brought
    /// back).
    parked: Option<Parked>,
}

/// The tab's run, if one.
#[derive(Default)]
pub(crate) struct ResolverState {
    run: Option<Run>,
    folded: bool,
    /// The merges put aside from this worktree (see `parking`).
    pub(in crate::repo) parked: Vec<Parked>,
    reading_parked: bool,
    /// The list changed (or was asked for) while it was read: that read is stale.
    read_parked_again: bool,
    /// The row to select and scroll to once the graph has it: the merge just put aside, or
    /// the changes of the one just brought back. Given up on after [`REVEAL_WITHIN`].
    reveal: Option<(Reveal, Instant)>,
}

/// A row the tab is to show once the graph has it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Reveal {
    Parked(String),
    Changes,
}

/// How long a row to reveal is waited for (a reload that never shows it).
const REVEAL_WITHIN: Duration = Duration::from_secs(10);

impl ResolverState {
    /// Whether a run is under way or waits for the user: another can't start.
    pub(crate) fn busy(&self) -> bool {
        self.run.is_some()
    }

    /// Whether the agent's work is shown for review in the big area.
    pub(crate) fn reviewing(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.review.is_some())
    }

    /// Whether the run goes on by itself: the agent works, or its files are being written.
    pub(crate) fn running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| {
            matches!(
                r.phase,
                Phase::Preparing | Phase::Working | Phase::Checking | Phase::Applying
            )
        })
    }

    /// The parked list was changed here: a read of it under way is stale.
    fn parked_changed(&mut self) {
        if self.reading_parked {
            self.read_parked_again = true;
        }
    }

    /// Whether the agent's work waits to be applied, and would go with the tab (a merge put
    /// aside keeps its own).
    pub(crate) fn waiting(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.phase == Phase::Ready && r.parked.is_none())
    }
}

/// What the tab says it is doing while the merge is put aside.
const PUTTING_ASIDE: &str = "Put the merge aside";

/// How often the card's clock moves.
const CLOCK: Duration = Duration::from_secs(1);

/// Removes what a run leaves, in the background (removing a worktree runs git): the agent is
/// stopped first, and only once it is gone is its folder removed (on Windows a running
/// program holds it).
fn clean_up(run: &mut Run) {
    let agent = run.agent.take();
    let workspace = run.workspace.take();
    let mcp_file = run.mcp_file.take();
    std::thread::spawn(move || {
        if let Some(agent) = agent {
            agent.stop_now();
        }
        if let Some(file) = mcp_file {
            let _ = std::fs::remove_file(file);
        }
        // Held elsewhere (being applied, or checked), it is removed when that ends: dropping
        // the last of it removes it too.
        if let Some(ws) = workspace.and_then(|ws| Arc::try_unwrap(ws).ok()) {
            let _ = ws.remove_patiently();
        }
    });
}

impl Drop for ResolverState {
    /// The tab is closed, or gitcraft is: the agent is stopped before this returns, as a
    /// thread left to do it may never run when the program ends. The rest is removed in the
    /// background (or, if gitcraft ends first, swept the next time it makes a scratch
    /// worktree).
    fn drop(&mut self) {
        if let Some(run) = &mut self.run {
            if let Some(agent) = run.agent.take() {
                agent.stop_now();
            }
            // The agent's work kept with a merge still put aside stays with it: the next
            // session shows it again (see `parking`). Only what is this session's goes.
            if run.parked.is_some() && run.workspace.as_ref().is_some_and(|w| w.is_kept()) {
                run.workspace = None;
            }
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
    /// The merge, put aside (when asked to and it could be).
    parked: Option<Parked>,
    /// Why it wasn't put aside, when it was asked to be.
    not_parked: Option<String>,
    /// The conflicted files behind the agent fence, left to the user.
    fenced: Vec<String>,
}

/// `files` split into those the agent may have and those behind the agent fence of `repo`
/// (see [`crate::fence`]).
fn split_by_fence(repo: &Path, files: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    let patterns = crate::fence::parse_list(&crate::fence::read(repo)?);
    Ok(files
        .iter()
        .cloned()
        .partition(|f| !patterns.iter().any(|p| p.matches(f))))
}

/// Why fenced files can't go to the agent.
fn behind_fence(fenced: &[String]) -> String {
    let (are, them) = if fenced.len() == 1 {
        ("is", "it")
    } else {
        ("are", "them")
    };
    format!(
        "{} {are} behind the agent fence (see Agent fence…): resolve {them} yourself",
        fenced.join(", ")
    )
}

/// Makes the agent's worktree and command for resolving `files`; then, when `put_aside`, puts
/// the merge aside (see `park`), the worktree free meanwhile. A merge that can't be (a rebase)
/// is resolved in place, and says why. Files behind the agent fence never reach the agent:
/// all of them are left to the user (the merge still put aside), and one asked for is refused.
fn prepare(
    repo: &Path,
    files: &[String],
    engine: Engine,
    id: &str,
    put_aside: bool,
) -> Result<Prepared, String> {
    let (open, fenced) = split_by_fence(repo, files)?;
    if !fenced.is_empty() && (!put_aside || open.is_empty()) {
        return Err(behind_fence(&fenced));
    }
    let exe = std::env::current_exe().map_err(|e| format!("gitcraft can't find itself: {e}"))?;
    let workspace = Workspace::prepare(repo, &open)?;
    let mcp_file = std::env::temp_dir().join(format!("gitcraft-resolve-{id}.json"));
    // The worktree goes with `workspace` on failure (dropping it removes it); the file here.
    let command = agent::command(engine, &exe, workspace.dir(), id, &mcp_file)?;
    let config = agent::claude_mcp_config(&exe, workspace.dir(), id);
    if let Err(e) = std::fs::write(&mcp_file, config.to_string()) {
        let _ = std::fs::remove_file(&mcp_file);
        return Err(format!("couldn't write the agent's MCP settings: {e}"));
    }
    let prompt = agent::prompt(&workspace, engine);
    // The tests' stand-in for the agent, when one is set.
    #[cfg(test)]
    let command = flow_tests::STAND_IN
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .unwrap_or(command);
    // Last: once the agent's copy of it is made, the merge is put aside.
    let (parked, not_parked) = match put_aside.then(|| park::park(repo, id, files)) {
        Some(Ok(parked)) => (Some(parked), None),
        Some(Err(why)) => (None, Some(why)),
        None => (None, None),
    };
    Ok(Prepared {
        workspace,
        mcp_file,
        command,
        prompt,
        parked,
        not_parked,
        fenced,
    })
}

/// The files the agent said it is done with: its steps reported done, by their file or id.
fn done_files(progress: &Progress) -> Vec<String> {
    progress
        .steps
        .iter()
        .filter(|s| s.status == crate::resolver::report::StepStatus::Done)
        .flat_map(|s| [Some(s.id.clone()), s.path.clone()])
        .flatten()
        .collect()
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
            // A merge put aside is brought back first: there is nothing to apply it to.
            ResolverAct::Apply
                if self
                    .resolver
                    .run
                    .as_ref()
                    .is_some_and(|r| r.parked.is_some()) => {}
            ResolverAct::Apply => self.apply_resolved(ctx),
            ResolverAct::BringBack(id) => self.bring_back(&id, ctx),
            ResolverAct::DropParked(id) => self.drop_parked(&id, ctx),
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
                // While files are written, the run stays to mark them resolved; a merge put
                // aside is never forgotten with it: it is brought back first.
                if self
                    .resolver
                    .run
                    .as_ref()
                    .is_some_and(|r| r.phase == Phase::Applying || r.parked.is_some())
                {
                    return;
                }
                if let Some(mut run) = self.resolver.run.take() {
                    clean_up(&mut run);
                }
                self.resolver.folded = false;
            }
            ResolverAct::Fold(folded) => self.resolver.folded = folded,
            ResolverAct::Review(i) => self.open_review(i, ctx),
            ResolverAct::ReviewAsDiff(on) => {
                if let Some(r) = self.resolver.run.as_mut().and_then(|r| r.review.as_mut()) {
                    r.as_diff = on;
                }
            }
            ResolverAct::CloseReview => {
                if let Some(run) = &mut self.resolver.run {
                    run.review = None;
                }
            }
        }
    }

    /// Shows the agent's work on its files, at the `i`th: read in the background the first
    /// time.
    fn open_review(&mut self, i: usize, ctx: &egui::Context) {
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Ready)
        else {
            return;
        };
        if let Some(review) = &mut run.review {
            review.selected = i;
            return;
        }
        let Some(ws) = run.workspace.clone() else {
            return;
        };
        run.review = Some(ReviewState {
            files: None,
            selected: i,
            as_diff: false,
        });
        let paths: Vec<String> = run.results.iter().map(|(p, _)| p.clone()).collect();
        let id = run.id.clone();
        self.then(
            ctx,
            move || {
                paths
                    .into_iter()
                    .map(|p| {
                        let r = review::of_file(&ws, &p);
                        (p, r)
                    })
                    .collect::<Vec<_>>()
            },
            move |tab, files, _| {
                let review = tab
                    .resolver
                    .run
                    .as_mut()
                    .filter(|r| r.id == id)
                    .and_then(|r| r.review.as_mut());
                if let Some(review) = review {
                    review.files = Some(files);
                }
            },
        );
    }

    /// Selects and shows the row the tab was waiting to (see `ResolverState::reveal`), once
    /// the graph read after the change has it.
    fn resolver_reveal(&mut self, ctx: &egui::Context) {
        let Some((want, since)) = self.resolver.reveal.clone() else {
            return;
        };
        if since.elapsed() > REVEAL_WITHIN {
            self.resolver.reveal = None;
            return;
        }
        if self.loading {
            return;
        }
        let row = match &want {
            Reveal::Parked(id) => graph::parked_row_of(&self.commits, id),
            Reveal::Changes => self
                .commits
                .iter()
                .position(|c| c.hash == crate::repo::WIP_HASH),
        };
        if let Some(row) = row {
            self.resolver.reveal = None;
            self.select(row, ctx);
            self.scroll_to = Some(row);
        }
    }

    /// Whether the run `id` is this tab's.
    pub(crate) fn resolver_owns(&self, id: &str) -> bool {
        self.resolver.run.as_ref().is_some_and(|r| r.id == id)
    }

    /// The view whose run `id` is: this one, or a worktree parked behind it.
    pub(crate) fn resolver_owner(&mut self, id: &str) -> Option<&mut RepoTab> {
        if self.resolver_owns(id) {
            return Some(self);
        }
        self.parked
            .iter_mut()
            .find(|p| p.resolver_owns(id))
            .map(|p| &mut **p)
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
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the running operation to finish".to_owned(), true));
            return;
        }
        // All of them: the merge is put aside meanwhile, the worktree free (one file is resolved
        // in place, from its conflict editor).
        let put_aside = files.is_none();
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
            unstaged: false,
            error: None,
            last_words: None,
            cost_usd: None,
            review: None,
            parked: None,
        });
        let repo = self.path.clone();
        let for_id = id.clone();
        // While the merge is put aside, nothing else runs on the worktree.
        if put_aside {
            self.busy_op = Some(PUTTING_ASIDE.to_owned());
        }
        self.then(
            ctx,
            move || prepare(&repo, &files, engine, &for_id, put_aside),
            move |tab, prepared, ctx| tab.resolver_prepared(&id, prepared, ctx),
        );
    }

    fn resolver_prepared(
        &mut self,
        id: &str,
        prepared: Result<Prepared, String>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.as_deref() == Some(PUTTING_ASIDE) {
            self.busy_op = None;
        }
        // The merge put aside: the worktree changed under the tab, and the graph has its row.
        let parked = prepared.as_ref().ok().and_then(|p| p.parked.clone());
        let not_parked = prepared.as_ref().ok().and_then(|p| p.not_parked.clone());
        let fenced = prepared
            .as_ref()
            .map(|p| p.fenced.clone())
            .unwrap_or_default();
        if let Some(parked) = &parked {
            self.resolver.parked.push(parked.clone());
            self.resolver.parked_changed();
            self.status = Some((
                format!(
                    "The {} is put aside while the agent resolves it: the worktree is free",
                    parked.what()
                ),
                false,
            ));
            self.resolver.reveal = Some((Reveal::Parked(parked.id.clone()), Instant::now()));
            self.reload(ctx);
        } else if let Some(why) = not_parked {
            self.status = Some((format!("Resolving in place: {why}"), false));
        }
        if !fenced.is_empty() {
            let said = self
                .status
                .as_ref()
                .map(|(s, _)| format!("{s}. "))
                .unwrap_or_default();
            self.status = Some((format!("{said}{}", behind_fence(&fenced)), false));
        }
        let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == id) else {
            // Dismissed meanwhile: what was made goes (a merge put aside stays, in the graph).
            if let Ok(p) = prepared {
                std::thread::spawn(move || {
                    let _ = std::fs::remove_file(&p.mcp_file);
                    let _ = p.workspace.remove();
                });
            }
            return;
        };
        run.parked = parked;
        run.files.retain(|f| !fenced.contains(f));
        let p = match prepared {
            Ok(p) if run.stopping => {
                // Stopped while it was being made: the agent isn't started.
                run.workspace = Some(Arc::new(p.workspace));
                run.mcp_file = Some(p.mcp_file);
                run.phase = Phase::Failed;
                run.error = Some("Stopped before the agent started".to_owned());
                run.ended = Some(Instant::now());
                clean_up(run);
                return;
            }
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

    /// Every frame the tab is shown: the row waited for is revealed; while an agent works, the
    /// card's clock moves.
    pub(super) fn resolver_frame(&mut self, ctx: &egui::Context) {
        self.resolver_reveal(ctx);
        if self
            .resolver
            .run
            .as_ref()
            .is_some_and(|r| matches!(r.phase, Phase::Preparing | Phase::Working))
        {
            ctx.request_repaint_after(CLOCK);
        }
    }

    /// With the tab's other work, shown or not (another tab's, a worktree parked behind): an
    /// agent that ended has its files looked at.
    pub(in crate::repo) fn resolver_poll(&mut self, ctx: &egui::Context) {
        let Some(run) = self
            .resolver
            .run
            .as_mut()
            .filter(|r| r.phase == Phase::Working)
        else {
            return;
        };
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
        // Its merge put aside: its work is kept with it, to bring back when the user is ready.
        if run.parked.is_some() {
            run.agent = None;
            self.keep_parked_work(ctx);
            return;
        }
        let Some(ws) = run.workspace.clone() else {
            run.phase = Phase::Failed;
            return;
        };
        run.phase = Phase::Checking;
        let (repo, id) = (self.path.clone(), run.id.clone());
        let done = done_files(&run.progress);
        self.then(
            ctx,
            move || ws.check(Some(&repo), &done),
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
}

#[cfg(test)]
mod look_tests;

#[cfg(test)]
mod flow_tests;
