//! The ghost run: hover (or hold Alt over) Rebase, Reset hard, Pull, Force push, Delete branch
//! or Drop commit, and the graph draws what it would do as translucent ghost commits, lanes
//! and pills over the real ones, with a strip listing the commits it would leave on no ref, the
//! files of the working tree that would change and where it would stop on a conflict. Nothing
//! is done: the result is worked out in the background (see [`plan`] and
//! [`crate::git::dry_run`]), after a short delay so a pointer passing by starts nothing,
//! cached by the commits involved, and dropped (its work stopped) when the hover ends.

mod failure;
mod request;
pub(crate) use failure::PreviewFailure;
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
    _scope: crate::tasks::Scope,
}

struct Shown {
    key: Key,
    /// The history the view was laid out from (see `RepoTab::history_gen`).
    history: u64,
    result: Arc<Result<Outcome, PreviewFailure>>,
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
    /// The refs' fingerprint, and the read of them it is of (see `RepoTab::history_gen`).
    refs_fp: Option<(u64, u64)>,
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

    /// For a pill standing for the refs `refs` (full names): whether one of them is drawn
    /// where the operation would put it, and whether the operation deletes them all.
    pub(crate) fn pill(&self, refs: &[String]) -> (bool, bool) {
        let Some(view) = self
            .shown
            .as_ref()
            .filter(|s| s.swapped)
            .and_then(|s| s.view.as_ref())
        else {
            return (false, false);
        };
        let moved = refs.iter().any(|r| view.moved.contains(r));
        let gone = !refs.is_empty() && refs.iter().all(|r| view.gone.contains(r));
        (moved, gone)
    }

    /// The hash of the tab's row `row` (numbered as the tab's own rows, now swapped out into
    /// the shown view).
    pub(crate) fn real_hash(&self, row: Option<usize>) -> Option<String> {
        let view = self.shown.as_ref().filter(|s| s.swapped)?.view.as_ref()?;
        Some(view.commits.get(row?)?.hash.clone())
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

    /// How many rows the ghost graph adds above the rows in view (scrolled away so those don't
    /// jump).
    pub(crate) fn rows_above(&self) -> usize {
        if self.shifted <= 0.0 {
            return 0;
        }
        self.shown
            .as_ref()
            .and_then(|s| s.view.as_ref())
            .map_or(0, |v| v.added)
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
        // A shift not made yet (the graph wasn't drawn meanwhile) is no longer wanted.
        if matches!(self.shift, Some(Shift::In(_))) {
            self.shift = None;
        }
        if self.shown.take().is_some() && self.shifted != 0.0 {
            self.shift = Some(Shift::Out(std::mem::take(&mut self.shifted)));
        }
    }

    fn cached(&self, key: &Key) -> Option<Arc<Result<Outcome, PreviewFailure>>> {
        self.cache
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, _, r)| Arc::clone(r))
    }

    fn remember(&mut self, key: Key, result: &Arc<Result<Outcome, PreviewFailure>>) {
        self.cache.retain(|(k, _, _)| *k != key);
        self.cache
            .push_front((key, Instant::now(), Arc::clone(result)));
        self.cache.truncate(CACHED);
    }
}

/// An outcome worked out, or why it couldn't be.
type Found = Arc<Result<Outcome, PreviewFailure>>;

/// What the strip shows.
pub(crate) enum Strip<'a> {
    /// Worked out in the background.
    Working(String),
    Ready(String, &'a Outcome),
    Failed(String, &'a PreviewFailure),
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
) -> Result<GitVersion, PreviewFailure> {
    if let Some(v) = cache.lock().ok().and_then(|c| *c) {
        return Ok(v);
    }
    let v = GitVersion::of_with_witness(repo)?;
    if let Ok(mut c) = cache.lock() {
        *c = Some(v);
    }
    Ok(v)
}

#[cfg(test)]
mod tests;

pub(in crate::repo) fn approval_preview_with_witness(
    repo: &std::path::Path,
    op: &GhostOp,
    facts: &op::Facts,
    cancelled: &dyn Fn() -> bool,
) -> Result<crate::repo::agent::preview::Preview, PreviewFailure> {
    let version = GitVersion::of_with_witness(repo)?;
    let outcome = plan::plan_with_witness(repo, op, facts, version, cancelled)?;
    Ok(crate::repo::agent::preview::Preview::from_outcome(outcome))
}

pub(in crate::repo) fn display_preview_note(raw: &str) -> String {
    plan::display_preview_text(raw)
}
