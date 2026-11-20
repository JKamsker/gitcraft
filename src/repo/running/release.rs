//! Operations that end outside `op_done` (a feature's own completion, or a forge operation
//! answering after the primary forge changed) still start what waited for them.

use super::super::{Msg, Notice, RepoTab};

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

    /// A forge operation (`reply`, never handled) answered after the primary forge changed.
    /// The mutation used the target it captured, so the busy slot is its own: it goes, but no
    /// continuation (or retry) runs against the new hosting.
    pub(in crate::repo) fn stale_forge_operation_ended(
        &mut self,
        reply: &Msg,
        ctx: &egui::Context,
    ) {
        self.stale_fork_operation_finished();
        let cancelled = self
            .running
            .as_ref()
            .is_some_and(|r| r.control.is_cancelled());
        // Create PR and stack publish answer with `OpDone`, whose result can be read. Fork
        // approval answers with a continuation that must not run here, so its outcome is not
        // known and it counts as done: not knowing is no reason to drop what waited.
        let failed = matches!(reply, Msg::OpDone { result: Err(_), .. });
        self.busy_op = None;
        self.op_started = None;
        self.running = None;
        let notice = Notice::plain(("The operation on the previous primary forge finished; refresh that repository to check its result".into(), true));
        self.status = Some(notice.clone());
        self.reload(ctx);
        // The same order as `op_done`, which also starts what waited before its reload lands.
        // That is safe for Fetch, Pull and Push: they work out what they run when they start,
        // from Git (`fetch --all`, `pull` of the upstream configured) or, for Push, from the
        // branch checked out, which this mutation of the forge's repository did not move; and
        // a push resolves its target again in the background before it runs.
        self.busy_op_released(!cancelled && !failed, ctx);
        // A waiter that started (or was refused) replaced the notice: it stays as a toast.
        if !self
            .status
            .as_ref()
            .is_some_and(|status| status.0.starts_with(&notice.0))
        {
            self.finished.push(notice);
        }
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
