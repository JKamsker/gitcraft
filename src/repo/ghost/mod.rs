//! The ghost run: hover (or hold Alt over) Rebase, Reset hard, Pull, Force push, Delete branch
//! or Drop commit, and the graph draws what it would do as translucent ghost commits, lanes
//! and pills over the real ones, with a strip listing the commits it would leave on no ref, the
//! files of the working tree that would change and where it would stop on a conflict. Nothing
//! is done: the result is worked out in the background (see [`plan`] and
//! [`crate::git::dry_run`]), after a short delay so a pointer passing by starts nothing,
//! cached by the commits involved, and dropped (its work stopped) when the hover ends.

pub(crate) mod op;
mod plan;
mod strip;
mod view;

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::Response;

use super::RepoTab;
use crate::git::Commit;
use crate::git::dry_run::{CANCELLED, GitVersion};
use crate::graph::Graph;
use op::Facts;
pub(crate) use op::GhostOp;
pub(crate) use plan::Outcome;
pub(crate) use view::{GhostView, Kind, RowGhost};

/// How long the pointer rests on an entry before its ghost run starts.
const HOVER_DELAY: Duration = Duration::from_millis(450);
/// With Alt held: at once, but not for every entry the pointer crosses on its way.
const ALT_DELAY: Duration = Duration::from_millis(80);
/// How many outcomes are kept, by the commits involved.
const CACHED: usize = 24;
/// How long a pull's outcome is kept: it depends on the remote, which moves on its own.
const PULL_FRESH: Duration = Duration::from_secs(60);

/// Offers `op` for a ghost run while `resp` (an enabled menu entry or button) is under the
/// pointer, for this frame.
pub(crate) fn offer(resp: &Response, op: impl FnOnce() -> GhostOp) {
    if resp.enabled() && resp.hovered() {
        let pass = resp.ctx.cumulative_pass_nr();
        let op = op();
        resp.ctx.data_mut(|d| d.insert_temp(offer_id(), (pass, op)));
    }
}

fn offer_id() -> egui::Id {
    egui::Id::new("ghost-run-offer")
}

/// The operation offered this frame, if any.
fn offered(ctx: &egui::Context) -> Option<GhostOp> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(u64, GhostOp)>(offer_id()))
        .filter(|(at, _)| *at == pass)
        .map(|(_, op)| op)
}

/// What an outcome is cached by: the operation and the commits the refs point at (and, for a
/// hard reset, the working tree as read).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    op: GhostOp,
    refs: u64,
}

struct Job {
    id: u64,
    key: Key,
    cancel: Arc<AtomicBool>,
}

struct Shown {
    key: Key,
    /// The history the view was laid out from (see `RepoTab::history_gen`).
    history: u64,
    result: Arc<Result<Outcome, String>>,
    view: Option<GhostView>,
    /// The view is in the tab's place while the graph draws (see [`Ghost::swap_in`]).
    swapped: bool,
}

/// A change of the graph's scroll offset the ghost rows ask for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shift {
    /// Rows added above: scrolled down by them, so the rows in view stay where they are.
    In(usize),
    /// Back up by what was scrolled.
    Out(f32),
}

/// The tab's ghost run.
#[derive(Default)]
pub(crate) struct Ghost {
    /// The entry under the pointer, since when.
    hover: Option<(GhostOp, Instant)>,
    job: Option<Job>,
    shown: Option<Shown>,
    cache: VecDeque<(Key, Instant, Found)>,
    version: Arc<Mutex<Option<GitVersion>>>,
    jobs: u64,
    shift: Option<Shift>,
    /// How far the graph was scrolled down for the ghost rows.
    pub(crate) shifted: f32,
}

impl Ghost {
    /// Whether the graph is drawing the ghost rows (between [`Self::swap_in`] and
    /// [`Self::swap_out`]).
    pub(crate) fn drawing(&self) -> bool {
        self.shown.as_ref().is_some_and(|s| s.swapped)
    }

    /// How row `i` of the ghost graph is drawn, while it is drawn.
    pub(crate) fn row(&self, i: usize) -> Option<&view::RowGhost> {
        let shown = self.shown.as_ref().filter(|s| s.swapped)?;
        shown.view.as_ref()?.row(i)
    }

    pub(crate) fn kind(&self, i: usize) -> Kind {
        self.row(i).map(|r| r.kind).unwrap_or_default()
    }

    /// Whether the pill named `label` is drawn where the operation would put it, and whether
    /// it is one the operation deletes.
    pub(crate) fn pill(&self, label: &str) -> (bool, bool) {
        let Some(view) = self
            .shown
            .as_ref()
            .filter(|s| s.swapped)
            .and_then(|s| s.view.as_ref())
        else {
            return (false, false);
        };
        (view.moved.contains(label), view.gone.contains(label))
    }

    /// Puts the ghost graph in the place of the tab's rows (`commits`, `graph`, `index`) for
    /// the graph to draw; `false` when there is none to draw.
    pub(crate) fn swap_in(
        &mut self,
        commits: &mut Vec<Arc<Commit>>,
        graph: &mut Graph,
        index: &mut std::collections::HashMap<String, usize>,
    ) -> bool {
        let Some(shown) = self.shown.as_mut().filter(|s| !s.swapped) else {
            return false;
        };
        let Some(view) = shown.view.as_mut() else {
            return false;
        };
        std::mem::swap(commits, &mut view.commits);
        std::mem::swap(graph, &mut view.graph);
        std::mem::swap(index, &mut view.index);
        shown.swapped = true;
        true
    }

    /// Puts the tab's rows back after [`Self::swap_in`].
    pub(crate) fn swap_out(
        &mut self,
        commits: &mut Vec<Arc<Commit>>,
        graph: &mut Graph,
        index: &mut std::collections::HashMap<String, usize>,
    ) {
        let Some(shown) = self.shown.as_mut().filter(|s| s.swapped) else {
            return;
        };
        if let Some(view) = shown.view.as_mut() {
            std::mem::swap(commits, &mut view.commits);
            std::mem::swap(graph, &mut view.graph);
            std::mem::swap(index, &mut view.index);
        }
        shown.swapped = false;
    }

    /// The scroll change the graph should make this frame.
    pub(crate) fn take_shift(&mut self) -> Option<Shift> {
        self.shift.take()
    }

    /// Ends the ghost run: its work is told to stop and what it showed goes.
    fn end(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.hover = None;
        if self.shown.take().is_some() && self.shifted != 0.0 {
            self.shift = Some(Shift::Out(std::mem::take(&mut self.shifted)));
        }
    }

    fn cached(&self, key: &Key) -> Option<Arc<Result<Outcome, String>>> {
        let fresh = |at: &Instant| !matches!(key.op, GhostOp::Pull(_)) || at.elapsed() < PULL_FRESH;
        self.cache
            .iter()
            .find(|(k, at, _)| k == key && fresh(at))
            .map(|(_, _, r)| Arc::clone(r))
    }

    fn remember(&mut self, key: Key, result: &Arc<Result<Outcome, String>>) {
        self.cache.retain(|(k, _, _)| *k != key);
        self.cache
            .push_front((key, Instant::now(), Arc::clone(result)));
        self.cache.truncate(CACHED);
    }
}

/// An outcome worked out, or why it couldn't be.
type Found = Arc<Result<Outcome, String>>;

/// What the strip shows.
pub(crate) enum Strip<'a> {
    /// Worked out in the background.
    Working(String),
    Ready(String, &'a Outcome),
    Failed(String, &'a str),
}

impl RepoTab {
    /// Once a frame, after drawing: starts, keeps or ends the ghost run of what the pointer is
    /// on.
    pub(super) fn ghost_frame(&mut self, ctx: &egui::Context) {
        let offered = offered(ctx).filter(|_| self.busy_op.is_none() && !self.loading);
        let Some(op) = offered else {
            if self.ghost.hover.is_some() || self.ghost.shown.is_some() {
                self.ghost.end();
                ctx.request_repaint();
            }
            return;
        };
        if self.ghost.hover.as_ref().map(|(o, _)| o) != Some(&op) {
            self.ghost.end();
            self.ghost.hover = Some((op.clone(), Instant::now()));
        }
        let waited = self
            .ghost
            .hover
            .as_ref()
            .map_or(Duration::ZERO, |(_, at)| at.elapsed());
        let due = if ctx.input(|i| i.modifiers.alt) {
            ALT_DELAY
        } else {
            HOVER_DELAY
        };
        if waited < due {
            ctx.request_repaint_after(due - waited);
            return;
        }
        let key = self.ghost_key(op);
        let history = self.history_gen;
        if let Some(shown) = &self.ghost.shown
            && shown.key == key
            && shown.history == history
        {
            return;
        }
        if self.ghost.job.as_ref().is_some_and(|j| j.key == key) {
            return;
        }
        self.start_ghost(key, ctx);
    }

    /// The key `op`'s outcome is cached by.
    fn ghost_key(&self, op: GhostOp) -> Key {
        let mut h = DefaultHasher::new();
        let refs = &self.refs;
        (&refs.head, &refs.current).hash(&mut h);
        for b in refs
            .local
            .iter()
            .chain(refs.tags.iter())
            .chain(refs.remotes.iter().flat_map(|(_, list)| list))
        {
            (&b.name, &b.target, &b.upstream).hash(&mut h);
        }
        self.work.is_clean().hash(&mut h);
        if matches!(op, GhostOp::ResetHard { .. }) {
            self.work_read.hash(&mut h);
        }
        Key {
            op,
            refs: h.finish(),
        }
    }

    /// Works out the outcome of `key`'s operation (or takes it from the cache) and lays out
    /// its ghost graph, in the background.
    fn start_ghost(&mut self, key: Key, ctx: &egui::Context) {
        if let Some(job) = self.ghost.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.ghost.jobs += 1;
        let id = self.ghost.jobs;
        let cancel = Arc::new(AtomicBool::new(false));
        self.ghost.job = Some(Job {
            id,
            key: key.clone(),
            cancel: Arc::clone(&cancel),
        });
        let cached = self.ghost.cached(&key);
        let facts = Facts {
            refs: self.refs.clone(),
            remotes: self.remotes.iter().map(|r| r.name.clone()).collect(),
            dirty: !self.work.is_clean(),
            credentials: self.credentials.clone(),
        };
        let rows = self.commits.clone();
        let repo = self.path.clone();
        let version = Arc::clone(&self.ghost.version);
        let history = self.history_gen;
        let op = key.op.clone();
        self.then(
            ctx,
            move || {
                let stop = || cancel.load(Ordering::Relaxed);
                let result = cached.unwrap_or_else(|| {
                    Arc::new(
                        git_version(&repo, &version)
                            .and_then(|v| plan::plan(&repo, &op, &facts, v, &stop)),
                    )
                });
                let head_ref = facts
                    .refs
                    .current
                    .as_deref()
                    .map(|c| format!("refs/heads/{c}"));
                let view = match (&*result, stop()) {
                    (Ok(outcome), false) => Some(view::build(&rows, outcome, head_ref.as_deref())),
                    _ => None,
                };
                (result, view)
            },
            move |tab, (result, view), ctx| tab.ghost_arrived(id, key, history, result, view, ctx),
        );
    }

    fn ghost_arrived(
        &mut self,
        id: u64,
        key: Key,
        history: u64,
        result: Arc<Result<Outcome, String>>,
        view: Option<GhostView>,
        ctx: &egui::Context,
    ) {
        let cancelled = matches!(&*result, Err(e) if e == CANCELLED);
        if !cancelled {
            self.ghost.remember(key.clone(), &result);
        }
        if self.ghost.job.as_ref().map(|j| j.id) != Some(id) || cancelled {
            return;
        }
        self.ghost.job = None;
        // Read again since it started: what it was laid out from is gone.
        if history != self.history_gen {
            self.start_ghost(key, ctx);
            return;
        }
        let added = view.as_ref().map_or(0, |v| v.added);
        if self.ghost.shifted != 0.0 {
            self.ghost.shift = Some(Shift::Out(std::mem::take(&mut self.ghost.shifted)));
        } else if added > 0 {
            self.ghost.shift = Some(Shift::In(added));
        }
        self.ghost.shown = Some(Shown {
            key,
            history,
            result,
            view,
            swapped: false,
        });
        ctx.request_repaint();
    }

    /// What the strip above the graph says, while a ghost run is on.
    pub(super) fn ghost_strip_state(&self) -> Option<Strip<'_>> {
        let (op, _) = self.ghost.hover.as_ref()?;
        let title = op.title(&self.refs);
        match &self.ghost.shown {
            Some(shown) if &shown.key.op == op => Some(match &*shown.result {
                Ok(outcome) => Strip::Ready(title, outcome),
                Err(e) => Strip::Failed(title, e),
            }),
            _ if self.ghost.job.is_some() => Some(Strip::Working(title)),
            _ => None,
        }
    }

    /// Whether row `i`'s commit is one a ghost run would make: it has no menu.
    pub(super) fn is_ghost_row(&self, i: usize) -> bool {
        self.ghost.kind(i) == Kind::New
    }
}

#[cfg(test)]
impl Ghost {
    /// Shows `outcome` over `rows` as if it had been worked out for a hover.
    pub(crate) fn show_for_test(&mut self, outcome: Outcome, rows: &[Arc<Commit>]) {
        let view = view::build(rows, &outcome, None);
        self.shown = Some(Shown {
            key: Key {
                op: GhostOp::DropCommit {
                    hash: String::new(),
                },
                refs: 0,
            },
            history: 0,
            result: Arc::new(Ok(outcome)),
            view: Some(view),
            swapped: false,
        });
    }
}

/// The git that runs for `repo`, asked once.
fn git_version(
    repo: &std::path::Path,
    cache: &Mutex<Option<GitVersion>>,
) -> Result<GitVersion, String> {
    if let Some(v) = cache.lock().ok().and_then(|c| *c) {
        return Ok(v);
    }
    let v = GitVersion::of(repo)?;
    if let Ok(mut c) = cache.lock() {
        *c = Some(v);
    }
    Ok(v)
}

#[cfg(test)]
mod tests;
