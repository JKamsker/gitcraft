use super::*;

impl RepoTab {
    /// Runs the rebase the editor describes.
    pub(in crate::repo) fn run_rebase(&mut self, ctx: &egui::Context) {
        let guard = self.same_branch();
        let Some(editor) = &mut self.rebase else {
            return;
        };
        if editor.problem().is_some() || editor.starting || self.busy_op.is_some() {
            return;
        }
        // Committed or pulled while the editor was open: its todo list would drop that. Checked
        // here against the last read, and again on disk right before it runs (that read can
        // lag a commit just made in the terminal by as long as a reload takes).
        if let Some(why) = editor.moved(&self.refs.head) {
            self.status = Some(crate::repo::Notice::plain((why, true)));
            return;
        }
        if editor.has_approval() {
            if !editor.approved(&guard, &self.refs.head) {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::text("repo-core-3341").into(),
                    true,
                )));
                return;
            }
        } else if let Err(reason) = editor.approve(&guard) {
            self.status = Some(crate::repo::Notice::plain((reason, true)));
            return;
        }
        let label = format!("Interactive rebase onto {}", editor.onto_label);
        // Started once: until its files are written (elsewhere, in the background), it can't
        // be started again, and it runs only if this editor is still the one open then.
        editor.starting = true;
        let (plan, id) = (editor.clone(), editor.id);
        if plan.update_refs {
            // The branches moved along are read with it: always in the background, it is more
            // git than the todo list alone.
            let repo = self.path.clone();
            self.then(
                ctx,
                move || Ok((plan.command_with_refs(&repo)?, plan.head)),
                move |tab, ready: Result<_, String>, ctx| {
                    let ready = ready.map(|((command, refs), head)| (command, head, refs));
                    tab.rebase_ready(id, &label, ready, ctx);
                },
            );
            return;
        }
        self.with_rebase_command(plan, ctx, move |tab, command, ctx| {
            let ready = command.map(|(command, head)| (command, head, Vec::new()));
            tab.rebase_ready(id, &label, ready, ctx);
        });
    }

    /// Runs the rebase of editor `id` once its command is ready, if that editor is still the
    /// one open: cancelled (or another rebase opened) meanwhile, nothing runs.
    pub(in crate::repo) fn rebase_ready(
        &mut self,
        id: u64,
        label: &str,
        command: Result<Ready, String>,
        ctx: &egui::Context,
    ) {
        let current_guard = self.same_branch();
        let Some(editor) = self.rebase.as_mut().filter(|e| e.id == id) else {
            return;
        };
        editor.starting = false;
        if !editor.approved(&current_guard, &self.refs.head) {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3343").into(),
                true,
            )));
            return;
        }
        let Some(retained_guard) = editor.branch_guard.clone() else {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3344").into(),
                true,
            )));
            return;
        };
        match command {
            // Something else started while the files were written.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-3345",
                        &[(
                            "label",
                            crate::repo::messages::display_operation(label).into(),
                        )],
                    ),
                    true,
                )));
            }
            Ok((command, head, refs)) => {
                if head != editor.head {
                    self.status = Some(crate::repo::Notice::plain((
                        crate::i18n::text("repo-core-3346").into(),
                        true,
                    )));
                    return;
                }
                let guards = [retained_guard.clone(), expect_ref_at("HEAD", &head)];
                self.rebase = None;
                let steps = [guards.to_vec(), vec![command]].concat();
                let undo = if refs.is_empty() {
                    Undo::HeadMove { mode: "--keep" }
                } else {
                    moved_refs_undo(retained_guard, &head, &refs, &steps)
                };
                self.run_undoable(label, steps, Refresh::All, undo, ctx);
            }
            Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
        }
    }
}
