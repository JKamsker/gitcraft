//! Operations that end outside `op_done` (a feature's own completion, or a forge operation
//! answering after the primary forge changed) still start what waited for them.

use super::super::RepoTab;

impl RepoTab {
    /// Once an operation that ends outside `op_done` has let go of the busy slot: starts what
    /// waited for it, or drops it after a failure, the same way `op_done` does (see
    /// [`Self::after_op`]).
    ///
    /// `ok` is false only for a failure the status line reports, since the waiters dropped are
    /// named there. A completion that shows its outcome elsewhere (its own window) passes true.
    /// While another operation holds the slot, such as the one a preparation step went on to
    /// start, this does nothing: that operation's own end starts them.
    pub(in crate::repo) fn busy_op_released(&mut self, ok: bool, ctx: &egui::Context) {
        if self.busy_op.is_none() {
            self.after_op(ok, false, ctx);
        }
    }
}
