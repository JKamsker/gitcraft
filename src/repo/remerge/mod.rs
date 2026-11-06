//! "What this merge resolved": a merge commit's remerge diff (see [`crate::git::remerge`]) in
//! the commit pane, and a badge on the graph's merge rows that resolved conflicts or edited
//! beyond the merge. Everything is read in the background, the badges only for rows shown.

mod badge;
mod panel;

pub(in crate::repo) use panel::Mode;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::git::{self, Remerge};
use crate::repo::RepoTab;

/// How many merges of the rows shown one background job redoes, one after the other: a
/// repository elsewhere answers each in a round trip, and nothing else waits behind them.
const BADGE_BATCH: usize = 4;

/// Merges kept read; past this, they are forgotten and read again when shown.
const KEPT: usize = 4_096;

#[derive(Default)]
pub(in crate::repo) struct Remerges {
    /// The commit pane shows what the merge resolved instead of the files it changed.
    pub(in crate::repo) on: bool,
    /// Whether this repository's git can redo merges, once asked; if not, why.
    support: Option<Result<(), String>>,
    checking: bool,
    /// The repository has merge drivers of its own, which redoing a merge runs: merges are
    /// then redone only when asked, not for the graph's badges.
    drivers: bool,
    /// What each merge read resolved, by its hash.
    found: HashMap<String, Result<Arc<Remerge>, String>>,
    /// Merges being read.
    asked: HashSet<String>,
    /// A job reading the badges of rows is running.
    batch_running: bool,
}

impl Remerges {
    /// Why the git of this repository can't show remerge diffs; `None` while that isn't known
    /// or when it can.
    pub(in crate::repo) fn unsupported(&self) -> Option<&str> {
        self.support.as_ref()?.as_ref().err().map(String::as_str)
    }

    fn supported(&self) -> bool {
        matches!(self.support, Some(Ok(())))
    }

    /// What the merge `hash` resolved, if it was read.
    pub(in crate::repo) fn of(&self, hash: &str) -> Option<&Result<Arc<Remerge>, String>> {
        self.found.get(hash)
    }

    /// The merges among `wanted` to read next for the graph's badges, now marked as asked.
    fn next_batch(&mut self, wanted: &[String]) -> Vec<String> {
        if self.batch_running || !self.supported() || self.drivers {
            return Vec::new();
        }
        let batch: Vec<String> = wanted
            .iter()
            .filter(|h| !self.found.contains_key(*h) && !self.asked.contains(*h))
            .take(BADGE_BATCH)
            .cloned()
            .collect();
        self.asked.extend(batch.iter().cloned());
        self.batch_running = !batch.is_empty();
        batch
    }

    /// Forgets what couldn't be read (the machine wasn't reached, the history was short…), to
    /// read it again: on a reload of the repository.
    pub(in crate::repo) fn forget_failures(&mut self) {
        self.found.retain(|_, found| found.is_ok());
        if matches!(self.support, Some(Err(_))) && !self.checking {
            self.support = None;
        }
    }

    /// Takes in what merge `hash` resolved.
    fn arrived(&mut self, hash: String, result: Result<Remerge, String>) {
        self.asked.remove(&hash);
        if self.found.len() >= KEPT {
            self.found.clear();
        }
        self.found.insert(hash, result.map(Arc::new));
    }
}

impl RepoTab {
    /// Asks once whether this repository's git can redo merges.
    fn want_remerge_support(&mut self, ctx: &egui::Context) {
        if self.remerges.support.is_some() || self.remerges.checking {
            return;
        }
        self.remerges.checking = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                guarded(|| {
                    git::remerge_support(&repo)?;
                    Ok(git::has_merge_drivers(&repo))
                })
            },
            |tab, found, _| {
                tab.remerges.checking = false;
                tab.remerges.drivers = found.as_ref().is_ok_and(|drivers| *drivers);
                tab.remerges.support = Some(found.map(|_| ()));
            },
        );
    }

    /// Reads what the merge `hash` (of two parents) resolved, unless it is read or being read.
    pub(in crate::repo) fn want_remerge(&mut self, hash: &str, ctx: &egui::Context) {
        self.want_remerge_support(ctx);
        let r = &mut self.remerges;
        if !r.supported() || r.found.contains_key(hash) || !r.asked.insert(hash.to_owned()) {
            return;
        }
        let repo = self.path.clone();
        let hash = hash.to_owned();
        self.then(
            ctx,
            move || {
                let result = guarded(|| git::remerge(&repo, &hash));
                (hash, result)
            },
            |tab, (hash, result), _| tab.remerges.arrived(hash, result),
        );
    }

    /// Reads, a few at a time, what the merges of two parents among the rows shown
    /// (`shown`) resolved, for their badges.
    pub(in crate::repo) fn want_remerge_badges(&mut self, shown: &[String], ctx: &egui::Context) {
        if shown.is_empty() {
            return;
        }
        self.want_remerge_support(ctx);
        let batch = self.remerges.next_batch(shown);
        if batch.is_empty() {
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                batch
                    .into_iter()
                    .map(|hash| {
                        let result = guarded(|| git::remerge(&repo, &hash));
                        (hash, result)
                    })
                    .collect::<Vec<_>>()
            },
            |tab, found, _| {
                tab.remerges.batch_running = false;
                for (hash, result) in found {
                    tab.remerges.arrived(hash, result);
                }
            },
        );
    }
}

/// `work`'s answer, or an error should it panic: what waits for it (the batch running, the
/// merge asked) is let go of either way.
fn guarded<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
        .unwrap_or_else(|_| Err("Internal error while redoing the merge".to_owned()))
}
