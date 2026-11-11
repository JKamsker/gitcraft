//! _Resolve with agent_: an agent resolves this checkout's conflicts in the background, in a
//! scratch worktree out of the user's way (see [`crate::resolver`]), while the tab shows its
//! plan, each step and what it is doing on a card of its own ([`panel`]). Once it is done the
//! user applies what it resolved: those files are written here and marked resolved.

mod failure;
use failure::*;
mod dispatch;
mod prepare;
use prepare::*;
mod lifecycle;
use lifecycle::*;
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

/// The files the agent said it is done with: its steps reported done, by their file or id.
fn done_files(progress: &Progress) -> Vec<String> {
    progress
        .steps
        .iter()
        .filter(|s| s.status == crate::resolver::report::StepStatus::Done)
        .map(|s| s.path.clone().unwrap_or_else(|| s.id.clone()))
        .collect()
}

impl RepoTab {
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
            self.status = Some(crate::repo::notice::Notice::plain((
                "Wait for the running operation to finish".to_owned(),
                true,
            )));
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
            self.status = Some(crate::repo::notice::Notice::plain((
                "There is no conflicted file to resolve".to_owned(),
                true,
            )));
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
            self.status = Some(crate::repo::notice::Notice::plain((
                format!(
                    "The {} is put aside while the agent resolves it: the worktree is free",
                    parked.what()
                ),
                false,
            )));
            self.resolver.reveal = Some((Reveal::Parked(parked.id.clone()), Instant::now()));
            self.reload(ctx);
        } else if let Some(why) = not_parked {
            self.status = Some(crate::repo::notice::Notice::plain((
                format!("Resolving in place: {why}"),
                false,
            )));
        }
        if !fenced.is_empty() {
            let said = self
                .status
                .as_ref()
                .map(|notice| format!("{}. ", notice.raw().0))
                .unwrap_or_default();
            self.status = Some(crate::repo::notice::Notice::plain((
                format!("{said}{}", behind_fence(&fenced)),
                false,
            )));
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
        let Some(exit) = run.agent.as_ref().and_then(|a| a.exit()) else {
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
        if !exit.safe_to_inspect || !run.agent.as_ref().is_some_and(|a| a.stop_now()) {
            confirm_cleanup(run);
            run.phase = Phase::Failed;
            run.picked.clear();
            run.error = Some(format!(
                "Agent cleanup could not be confirmed: {}",
                exit.stderr
            ));
            return;
        }
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
