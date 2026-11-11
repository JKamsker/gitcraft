//! Absorb read/check/run lifecycle and git-absorb fallback.

use super::*;

impl RepoTab {
    /// Reads what is staged and the branch's commits, and opens the absorb editor.
    pub(in crate::repo) fn open_absorb(&mut self, ctx: &egui::Context) {
        let Some(branch) = self.refs.current.clone() else {
            self.status = Some(crate::repo::Notice::plain((
                "Distribute staged fixes: HEAD is detached, check out a branch first".to_owned(),
                true,
            )));
            return;
        };
        if let Some(state) = self.work.in_progress {
            self.status = Some(crate::repo::Notice::plain((
                in_progress_status(state),
                true,
            )));
            return;
        }
        if self.work.staged.is_empty() {
            self.status = Some(crate::repo::Notice::plain((
                "Distribute staged fixes: nothing is staged, stage the fixes first".to_owned(),
                true,
            )));
            return;
        }
        if self.split.is_some() || self.rebase.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Distribute staged fixes: close the editor that is open first".to_owned(),
                true,
            )));
            return;
        }
        self.status = Some(crate::repo::Notice::plain((
            "Distribute staged fixes: reading…".to_owned(),
            false,
        )));
        let path = self.path.clone();
        self.then(
            ctx,
            move || read(&path, &branch),
            |tab, result, _| match result {
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() || tab.split.is_some() => {
                    tab.status = Some(crate::repo::Notice::plain((
                        "Distribute staged fixes: another editor opened meanwhile".to_owned(),
                        true,
                    )));
                }
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    tab.absorb = Some(Box::new(AbsorbEditor::new(read)));
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Distribute staged fixes: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    pub(in crate::repo) fn check_absorb(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.absorb.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        let (files, table, targets) = (
            editor.files.clone(),
            editor.table(),
            editor.chosen_targets(),
        );
        let names: Vec<String> = editor.targets.iter().map(|t| t.hash.clone()).collect();
        let (id, head) = (editor.id, editor.head.clone());
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let (each, all, staged) = patches(&files, &table, &targets);
                prepare(&path, &head, &each, &all, &staged, &names)
            },
            move |tab, result, _| {
                let Some(editor) = tab.absorb.as_mut().filter(|e| e.id == id) else {
                    return;
                };
                editor.busy = false;
                match result {
                    Ok(checked) => editor.checked = Some(checked),
                    Err(e) => editor.error = Some(e),
                }
            },
        );
    }

    /// Moves the branch to the commits checking made, the fixes in them. What is staged and
    /// the working tree are left as they are: what was absorbed is in HEAD now, so only what
    /// wasn't shows staged. It refuses once HEAD, the branch or what is staged changed since.
    pub(in crate::repo) fn run_absorb(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(editor) = self.absorb.as_ref().filter(|e| !e.busy) else {
            return;
        };
        let Some(checked) = editor.checked.clone() else {
            return;
        };
        let label = format!(
            "Distribute staged fixes into {} commit{}",
            checked.fixups.len(),
            if checked.fixups.len() == 1 { "" } else { "s" }
        );
        let steps = vec![
            self.same_branch(),
            expect_ref_at("HEAD", &editor.head),
            ["diff", "--cached", "--quiet", &checked.staged, "--"]
                .map(str::to_owned)
                .to_vec(),
            [
                "update-ref",
                "-m",
                "gitcraft: distribute staged fixes",
                "HEAD",
                &checked.new_head,
                &editor.head,
            ]
            .map(str::to_owned)
            .to_vec(),
        ];
        self.absorb = None;
        self.run_undoable(
            &label,
            steps,
            crate::repo::Refresh::All,
            Undo::HeadMove { mode: "--soft" },
            ctx,
        );
    }

    /// Runs the installed `git-absorb` instead: it makes the fixup commits and rebases them in.
    pub(in crate::repo) fn run_git_absorb(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.absorb.as_ref().filter(|e| !e.busy) else {
            return;
        };
        let steps = [
            self.same_branch(),
            expect_ref_at("HEAD", &editor.head),
            // Its rebase takes the todo list as it is, whatever editor the user's config sets.
            ["-c", "sequence.editor=true", "absorb", "--and-rebase"]
                .map(str::to_owned)
                .to_vec(),
        ]
        .to_vec();
        self.absorb = None;
        self.run_undoable(
            "git absorb",
            steps,
            crate::repo::Refresh::All,
            Undo::HeadMove { mode: "--soft" },
            ctx,
        );
    }
}
