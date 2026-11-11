use super::*;

impl RepoTab {
    /// Escape closes the innermost view; Ctrl+Z / Ctrl+Y undo and redo, Ctrl+Shift+Z shows the
    /// undo tree, Ctrl+F searches, F7 and
    /// Shift+F7 (or Alt+↓ / Alt+↑) go to the next and previous change of a diff; the keys of
    /// [`crate::keymap`] fetch, pull, push, stash, branch, stage, commit and open the terminal.
    /// None of them while a dialog, a text field or the terminal has the keyboard.
    pub(in crate::repo) fn shortcuts(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let free_keys = self.dialog.is_none()
            && self.package.is_none()
            && self.release.is_none()
            && self.hotfix.finish.is_none()
            && self.agents.dialog.is_none()
            && self.bakeoff.picker.is_none()
            && !self.stacks.modal_open()
            && !self.issue_picker_open()
            && !self.email.open
            && !self.devcontainer.open
            && !self.keys_blocked
            && !crate::a11y::keyboard_taken(ctx);
        // Ctrl+F, Esc and the steps of the find bar, when a diff, file or blame is shown.
        self.find_keys(ctx, free_keys);
        if free_keys {
            let commands = keymap::pressed(ctx, keymap::Scope::Repository);
            // While a commit's message is being edited, Ctrl+Enter saves it (the editor takes
            // the key also when nothing has the focus): it doesn't commit the changes too.
            let rewording = self.reword.is_some();
            acts.extend(
                commands
                    .into_iter()
                    .filter(|&c| !(rewording && c == keymap::Command::Commit))
                    .map(Action::Key),
            );
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.close_innermost_view(ctx, acts);
            }
            for command in keymap::pressed(ctx, keymap::Scope::Changes) {
                acts.push(Action::Key(command));
            }
        } else {
            keymap::retain_allowed(ctx);
        }
        if !free_keys && self.search.open && keymap::take(ctx, keymap::Command::Search) {
            self.search.focus = true;
        }
    }
    /// Close priority follows the central rendering stack; nested comments win first.
    fn close_innermost_view(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let diff_keys =
            self.diff.is_some() && (self.main_keys(ctx, Panel::Diff) || self.conflict.is_some());
        if self.file_comparison.is_some() {
            acts.push(Action::CloseFileComparison);
        } else if self.close_drop_menu() {
        } else if self.resolver.reviewing() {
            // Drawn over everything else of the big area (a conflict editor too): it
            // closes first, and the editor under it stays as it is.
            acts.push(Action::Resolver(
                crate::repo::resolver::ResolverAct::CloseReview,
            ));
        } else if diff_keys && self.note_open(ctx) {
            acts.push(Action::Agent(crate::repo::agents::AgentAct::Tray(
                crate::repo::agents::TrayAct::Close,
            )));
        } else if diff_keys && self.line_comment_open() {
            acts.push(Action::Pr(pr_review::PrAct::CloseLine));
        } else if self.diff.as_ref().is_some_and(|d| d.moved.is_some()) {
            acts.push(Action::DiffMoved(None));
        } else if self.conflict.is_none() && self.rebase.as_ref().is_some_and(|r| !r.typing) {
            // What is drawn first (see the central panel) closes first: a diff opened
            // meanwhile is under it.
            acts.push(Action::CloseRebase);
        } else if self.conflict.is_none() && self.split.is_some() {
            acts.push(Action::CloseSplit);
        } else if self.conflict.is_none() && self.absorb.is_some() {
            acts.push(Action::CloseAbsorb);
        } else if diff_keys {
            acts.push(Action::CloseDiff);
        } else if let Some(rebase) = &self.rebase {
            if !rebase.typing {
                acts.push(Action::CloseRebase);
            }
        } else if self.file_view.is_some() {
            acts.push(Action::CloseFileView);
        } else if self.bakeoff.view.is_some() {
            acts.push(Action::Bake(crate::repo::bakeoff::BakeAct::Close));
        } else if self.ci_view.is_some() && self.main_keys(ctx, Panel::Ci) {
            acts.push(Action::CloseCi);
        } else if self.versions.view.is_some() {
            // The patch of a pair first, then the comparison.
            let open_pair = self
                .versions
                .view
                .as_ref()
                .is_some_and(|v| v.selected.is_some());
            acts.push(Action::Versions(if open_pair {
                crate::repo::versions::VersionsAct::Deselect
            } else {
                crate::repo::versions::VersionsAct::Close
            }));
        } else if self.pr_view.is_some() && self.main_keys(ctx, Panel::Pr) {
            acts.push(Action::ClosePr);
        } else if self.timeline_view.is_some() {
            acts.push(Action::ToggleTimeline);
        } else if self.insights.is_some() {
            acts.push(Action::Insights(crate::repo::insights::InsightsAct::Back));
        } else if self.doctor.as_ref().is_some_and(|d| !d.asking()) {
            acts.push(Action::CloseDoctor);
        } else if self.compare.is_some() {
            acts.push(Action::CloseCompare);
        } else if self.search.open {
            acts.push(Action::ToggleSearch);
        } else if self.reflog.is_some() {
            acts.push(Action::CloseReflog);
        }
    }
}
