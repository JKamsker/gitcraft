//! On-demand resource state; dropping or refreshing retires results and cancels work.
mod jobs;
mod preview;
pub(super) mod rows;
use crate::git::worktree_resources::Inventory;
use crate::repo::insights::common::Slot;
#[derive(Default)]
pub(super) struct State {
    result: Slot<Inventory>,
    scope: crate::tasks::Scope,
    path: String,
    preview: bool,
}
impl State {
    pub fn stop(&mut self) {
        self.scope.0.cancel();
        self.result.restart();
        self.result.loading = false;
        self.preview = false;
    }
}
