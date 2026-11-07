//! Conflict archaeology: the conflict editor shows each side's commits that touched the file,
//! and _Find where it started_ looks for the first pair of them whose merge conflicts in the
//! file, by test merges (git-imerge style), in the background, bounded and cancelable. A commit
//! picked on each side can be test-merged too.

mod git;
mod search;
mod view;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub use self::git::{Step, Timelines};
use self::search::{Outcome, Stop};
use crate::git::InProgress;
use crate::repo::RepoTab;

/// Something being read in the background, read, or not read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Read<T> {
    #[default]
    NotAsked,
    Reading,
    Done(T),
    Failed(String),
}

/// The pair of commits that first made the two sides conflict, or why there is none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// Indexes in [`Timelines::ours`] and [`Timelines::theirs`]; `checked`: the commits just
    /// before each merge cleanly with the other (see [`search::Outcome::Pair`]).
    Pair {
        ours: usize,
        theirs: usize,
        checked: bool,
    },
    OneSided,
    TipsMergeCleanly,
}

/// A search for the first conflicting pair, running or done.
#[derive(Debug)]
pub struct Search {
    /// Set to stop it before its next test merge.
    pub cancel: Arc<AtomicBool>,
    /// How many test merges it has run.
    pub merges: Arc<AtomicUsize>,
    pub result: Read<Found>,
}

/// What the conflict editor shows of the file's history on each side.
#[derive(Debug, Default)]
pub struct History {
    pub timelines: Read<Timelines>,
    pub search: Option<Search>,
    /// The commit picked on each side (indexes).
    pub picked: (Option<usize>, Option<usize>),
    /// Whether the two picked conflict, for the pair it was asked for.
    pub pair_test: Option<((usize, usize), Read<bool>)>,
    /// The pairs being test-merged: not merged again while they are.
    pub testing: Vec<(usize, usize)>,
    /// The change of a commit to the file, shown in place of the timelines: its hash, its
    /// short hash, and it.
    pub change: Option<(String, String, Read<String>)>,
}

impl History {
    /// Stops the search, if one runs: at once for the view, before its next test merge for
    /// git.
    pub fn cancel(&mut self) {
        if let Some(search) = self.search.take() {
            search.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Whether a search runs.
    pub fn searching(&self) -> bool {
        self.search
            .as_ref()
            .is_some_and(|s| s.result == Read::Reading)
    }
}

impl Drop for History {
    /// A search for an editor closed (or opened again) since stops: nobody waits for it.
    fn drop(&mut self) {
        self.cancel();
    }
}

/// What the views ask of the archaeology.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Read each side's commits (again, after a failure).
    Load,
    /// Look for the first conflicting pair.
    Find,
    /// Test-merge the commits picked on each side (indexes).
    TestPair(usize, usize),
    /// Show (or hide) what a commit changed in the file: its hash, its short hash.
    ShowChange(String, String),
}

/// Whether the archaeology can look into the operation `state` stopped at (a stash or a patch
/// brings no other branch).
pub fn applies(state: Option<InProgress>) -> bool {
    matches!(
        state,
        Some(InProgress::Merge | InProgress::Rebase | InProgress::CherryPick | InProgress::Revert)
    )
}

/// The search, on what `timelines` lists: blocking, from the background. Each test merge
/// first checks `cancel`, and counts itself in `merges`.
fn find(
    repo: &std::path::Path,
    path: &str,
    timelines: &Timelines,
    (cancel, merges): (&AtomicBool, &AtomicUsize),
    ctx: &egui::Context,
) -> Result<Found, Stop> {
    let base = timelines.base.as_deref();
    let outcome = search::first_pair(timelines.ours.len(), timelines.theirs.len(), |i, j| {
        if cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let (Some(a), Some(b)) = (timelines.ours.get(i - 1), timelines.theirs_merged(j - 1)) else {
            return Err(Stop::Failed("the commits changed meanwhile".to_owned()));
        };
        let conflicts = git::merge_conflicts(repo, (&a.hash, b), base, path).map_err(Stop::Failed);
        merges.fetch_add(1, Ordering::Relaxed);
        ctx.request_repaint();
        conflicts
    })?;
    Ok(match outcome {
        Outcome::Pair {
            ours,
            theirs,
            checked,
        } => Found::Pair {
            ours: ours - 1,
            theirs: theirs - 1,
            checked,
        },
        Outcome::OneSided => Found::OneSided,
        Outcome::TipsMergeCleanly => Found::TipsMergeCleanly,
    })
}

impl RepoTab {
    /// The open conflict editor's history, if it is still the one opened as `generation`.
    fn history_for(&mut self, generation: u64, path: &str) -> Option<&mut History> {
        self.conflict
            .as_mut()
            .filter(|ed| ed.generation == generation && ed.path == path)
            .map(|ed| &mut ed.history)
    }

    pub(in crate::repo) fn archaeology_act(&mut self, act: Act, ctx: &egui::Context) {
        let repo = self.path.clone();
        let state: Option<InProgress> = self.work.in_progress;
        let Some(ed) = self.conflict.as_mut() else {
            return;
        };
        let (generation, path) = (ed.generation, ed.path.clone());
        let history = &mut ed.history;
        match act {
            Act::Load => {
                if matches!(history.timelines, Read::Reading | Read::Done(_)) {
                    return;
                }
                history.timelines = Read::Reading;
                let file = path.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || git::timelines(&repo, &file, state),
                    move |tab, read, _| {
                        if let Some(h) = tab.history_for(generation, &path) {
                            h.timelines = match read {
                                Ok(t) => Read::Done(t),
                                Err(e) => Read::Failed(e),
                            };
                        }
                    },
                );
            }
            Act::Find => {
                let Read::Done(timelines) = &history.timelines else {
                    return;
                };
                if history.searching() {
                    return;
                }
                let timelines = timelines.clone();
                let (cancel, merges) = (Arc::new(AtomicBool::new(false)), Arc::default());
                history.search = Some(Search {
                    cancel: Arc::clone(&cancel),
                    merges: Arc::clone(&merges),
                    result: Read::Reading,
                });
                let (file, repaint, mine) = (path.clone(), ctx.clone(), Arc::clone(&cancel));
                self.then_or(
                    ctx,
                    false,
                    |error| Err(Stop::Failed(error)),
                    move || find(&repo, &file, &timelines, (&cancel, &merges), &repaint),
                    move |tab, found, _| {
                        // Only for this search: not one cancelled since, nor one started after.
                        let Some(search) = tab
                            .history_for(generation, &path)
                            .and_then(|h| h.search.as_mut())
                            .filter(|s| Arc::ptr_eq(&s.cancel, &mine))
                        else {
                            return;
                        };
                        search.result = match found {
                            Ok(f) => Read::Done(f),
                            Err(Stop::Cancelled) => Read::NotAsked,
                            Err(Stop::Failed(e)) => Read::Failed(e),
                        };
                    },
                );
            }
            Act::TestPair(i, j) => {
                let Read::Done(timelines) = &history.timelines else {
                    return;
                };
                let (Some(a), Some(b)) = (timelines.ours.get(i), timelines.theirs_merged(j)) else {
                    return;
                };
                let (a, b, base) = (a.hash.clone(), b.to_owned(), timelines.base.clone());
                // Already being tested: shown as such, not merged a second time.
                let again = history.testing.contains(&(i, j));
                history.pair_test = Some(((i, j), Read::Reading));
                if again {
                    return;
                }
                history.testing.push((i, j));
                let file = path.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || git::merge_conflicts(&repo, (&a, &b), base.as_deref(), &file),
                    move |tab, conflicts, _| {
                        let Some(h) = tab.history_for(generation, &path) else {
                            return;
                        };
                        h.testing.retain(|p| *p != (i, j));
                        if let Some(test) = h.pair_test.as_mut().filter(|(pair, _)| *pair == (i, j))
                        {
                            test.1 = match conflicts {
                                Ok(c) => Read::Done(c),
                                Err(e) => Read::Failed(e),
                            };
                        }
                    },
                );
            }
            Act::ShowChange(hash, short) => {
                if history.change.as_ref().is_some_and(|(h, _, _)| *h == hash) {
                    history.change = None;
                    return;
                }
                history.change = Some((hash.clone(), short, Read::Reading));
                let (file, shown) = (path.clone(), hash.clone());
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || git::change(&repo, &shown, &file),
                    move |tab, read, _| {
                        if let Some(change) = tab
                            .history_for(generation, &path)
                            .and_then(|h| h.change.as_mut())
                            .filter(|(h, _, _)| *h == hash)
                        {
                            change.2 = match read {
                                Ok(text) => Read::Done(text),
                                Err(e) => Read::Failed(e),
                            };
                        }
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
