//! On-demand observations and asynchronously verified preview presentation.
mod jobs;
mod preview;
pub(super) mod rows;
#[cfg(test)]
mod tests;
use crate::git::worktree_resources::Inventory;
use crate::repo::insights::common::Slot;

#[derive(Clone)]
pub(super) struct DiskObservation {
    pub size: crate::insights::constellation::read::Disk,
    pub head: String,
    pub fingerprint: String,
    pub observed: i64,
}
#[derive(Default)]
pub(super) struct State {
    result: Slot<Inventory>,
    validation: Slot<Inventory>,
    scope: crate::tasks::Scope,
    path: String,
    preview: bool,
    checked: Option<std::time::Instant>,
}
impl State {
    pub fn stop(&mut self) {
        self.scope.0.cancel();
        self.result.restart();
        self.result.loading = false;
        self.validation.restart();
        self.validation.loading = false;
        self.preview = false;
        self.checked = None;
    }

    /// A fresh worker observation must match the displayed inventory before roots appear.
    fn verified(&mut self, request: u64, fresh: Result<Inventory, String>) -> Option<bool> {
        if !self.validation.accept(request, fresh) {
            return None;
        }
        let matches = self
            .result
            .ready()
            .zip(self.validation.ready())
            .is_some_and(|(old, new)| {
                old.complete()
                    && new.complete()
                    && !old.fingerprint.is_empty()
                    && old.fingerprint == new.fingerprint
                    && old.root == new.root
                    && old.head == new.head
                    && old.branch == new.branch
                    && old.presence == new.presence
            });
        self.preview = matches;
        self.checked = matches.then(std::time::Instant::now);
        if !matches {
            let id = self.result.restart();
            self.result
                .accept(id, Err(crate::i18n::text("resources-stale").into()));
        }
        Some(matches)
    }
}
