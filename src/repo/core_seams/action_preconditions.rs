use super::*;

impl RepoTab {
    pub(in crate::repo) fn prepare_repo_action(
        &mut self,
        action: Action,
        ctx: &egui::Context,
    ) -> Option<Action> {
        // Cached startup state may be browsed, but cannot authorize an operation.
        // Default to refusal for every other action, including feature-specific mutations.
        if self.snapshot_pending()
            && !matches!(
                &action,
                Action::Select(_)
                    | Action::SelectHash(_)
                    | Action::OpenCommitChanges(_)
                    | Action::OpenDiff(_)
                    | Action::CloseDiff
                    | Action::FileView { .. }
                    | Action::CloseFileView
                    | Action::Reload
            )
        {
            return None;
        }
        self.notice_connections(ctx);
        let action = match action {
            Action::OpenLinkedPr { number, remote } => {
                if !self.select_linked_forge(&remote, ctx) {
                    return None;
                }
                Action::OpenPr(number)
            }
            action => action,
        };
        let action = self.popout_intercept(action, ctx)?;
        // Any later user action supersedes a pending automatic backlink diff opening.
        if !matches!(&action, Action::OpenCommitChanges(_)) {
            self.commit_changes.cancel_pending();
        }
        if matches!(
            &action,
            Action::CloseDiff | Action::OpenDiff(_) | Action::OpenCommitChanges(_)
        ) {
            self.commit_changes.stop();
        }
        if matches!(
            &action,
            Action::OpenDiff(_)
                | Action::OpenCommitChanges(_)
                | Action::FileView { .. }
                | Action::ShowActivity(_)
                | Action::OpenPr(_)
                | Action::OpenCi { .. }
        ) {
            self.dossier.close();
            self.symbols.close();
        }
        if matches!(
            &action,
            Action::Select(_)
                | Action::SelectHash(_)
                | Action::SelectStash(_)
                | Action::OpenPr(_)
                | Action::ClosePr
                | Action::Compare { .. }
                | Action::CloseCompare
        ) {
            self.keyboard_clear_cursor();
        }
        // Fetch, Pull and Push asked for while something runs start once it is done.
        if self.busy_op.is_some()
            && let Some(waiter) = crate::repo::running::Waiter::of(&action, ctx)
        {
            self.wait_for_running(waiter);
            return None;
        }
        let action = self.fence_gate(action, ctx)?;
        if matches!(
            &action,
            Action::OpenDiff(_)
                | Action::OpenCommitChanges(_)
                | Action::OpenPr(_)
                | Action::OpenCi { .. }
                | Action::FileView { .. }
        ) {
            self.file_comparison = None;
        }
        if matches!(
            &action,
            Action::OpenDiff(_)
                | Action::FileView { .. }
                | Action::OpenPr(_)
                | Action::Select(_)
                | Action::SelectHash(_)
                | Action::SelectPr(_)
                | Action::SelectStash(_)
                | Action::Compare { .. }
        ) {
            self.review_document.open = false;
        }
        Some(action)
    }
}
