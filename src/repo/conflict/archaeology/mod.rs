//! Conflict archaeology: the conflict editor shows each side's commits that touched the file
//! since the two diverged, and _Find where it started_ looks for the first pair of them whose
//! merge conflicts in the file, by test merges (git-imerge style), in the background,
//! bounded and cancelable.

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
    /// Indexes in [`Timelines::ours`] and [`Timelines::theirs`].
    Pair {
        ours: usize,
        theirs: usize,
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
    /// Folded away.
    pub folded: bool,
    /// The change of a commit to the file, shown under the timelines (its hash, and it).
    pub change: Option<(String, Read<String>)>,
}

impl History {
    /// Stops the search, if one runs.
    pub fn cancel(&self) {
        if let Some(search) = &self.search {
            search.cancel.store(true, Ordering::Relaxed);
        }
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
    /// Read each side's commits.
    Load,
    /// Look for the first conflicting pair.
    Find,
    /// Show (or hide) what a commit changed in the file.
    ShowChange(String),
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
    let outcome = search::first_pair(timelines.ours.len(), timelines.theirs.len(), |i, j| {
        if cancel.load(Ordering::Relaxed) {
            return Err(Stop::Cancelled);
        }
        let (Some(a), Some(b)) = (timelines.ours.get(i - 1), timelines.theirs.get(j - 1)) else {
            return Err(Stop::Failed("the commits changed meanwhile".to_owned()));
        };
        let conflicts = git::merge_conflicts(repo, &a.hash, &b.hash, path).map_err(Stop::Failed);
        merges.fetch_add(1, Ordering::Relaxed);
        ctx.request_repaint();
        conflicts
    })?;
    Ok(match outcome {
        Outcome::Pair { ours, theirs } => Found::Pair {
            ours: ours - 1,
            theirs: theirs - 1,
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
                if history.timelines == Read::Reading {
                    return;
                }
                history.timelines = Read::Reading;
                let file = path.clone();
                self.then(
                    ctx,
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
                let timelines = timelines.clone();
                if history
                    .search
                    .as_ref()
                    .is_some_and(|s| s.result == Read::Reading)
                {
                    return;
                }
                let (cancel, merges) = (Arc::new(AtomicBool::new(false)), Arc::default());
                history.search = Some(Search {
                    cancel: cancel.clone(),
                    merges: Arc::clone(&merges),
                    result: Read::Reading,
                });
                let (file, repaint) = (path.clone(), ctx.clone());
                self.then(
                    ctx,
                    move || find(&repo, &file, &timelines, (&cancel, &merges), &repaint),
                    move |tab, found, _| {
                        if let Some(search) = tab
                            .history_for(generation, &path)
                            .and_then(|h| h.search.as_mut())
                        {
                            search.result = match found {
                                Ok(f) => Read::Done(f),
                                Err(Stop::Cancelled) => Read::NotAsked,
                                Err(Stop::Failed(e)) => Read::Failed(e),
                            };
                        }
                    },
                );
            }
            Act::ShowChange(hash) => {
                if history.change.as_ref().is_some_and(|(h, _)| *h == hash) {
                    history.change = None;
                    return;
                }
                history.change = Some((hash.clone(), Read::Reading));
                let file = path.clone();
                let shown = hash.clone();
                self.then(
                    ctx,
                    move || git::change(&repo, &shown, &file),
                    move |tab, read, _| {
                        if let Some(change) = tab
                            .history_for(generation, &path)
                            .and_then(|h| h.change.as_mut())
                            .filter(|(h, _)| *h == hash)
                        {
                            change.1 = match read {
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
