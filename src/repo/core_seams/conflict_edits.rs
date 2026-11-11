use super::*;

impl RepoTab {
    /// Turns stripping notebooks' outputs when committing on or off, then shows the diff again.
    pub(in crate::repo) fn strip_notebook_outputs(&mut self, on: bool, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let done = git::set_strip_outputs(&path, on);
                (done, git::strip_state(&path))
            },
            move |tab, (result, state), ctx| {
                tab.status = Some(crate::repo::Notice::plain(match result {
                    Ok(Some(command)) => {
                        let tool = if command == "nbstripout" {
                            "nbstripout"
                        } else {
                            "Python"
                        };
                        (
                            crate::i18n::message(
                                "repo-core-0172",
                                &[("tool", tool.to_string().into())],
                            ),
                            false,
                        )
                    }
                    Ok(_) => match state {
                        git::StripState::Other(name) => (
                            crate::i18n::message(
                                "repo-core-0173",
                                &[("name", name.to_string().into())],
                            ),
                            false,
                        ),
                        _ => (crate::i18n::text("repo-core-0174").to_owned(), false),
                    },
                    Err(e) => (
                        crate::i18n::message("repo-core-0175", &[("e", e.to_string().into())]),
                        true,
                    ),
                }));
                tab.reload_status(ctx);
                tab.refresh_diff(ctx);
            },
        );
    }

    /// Asks Mergiraf to solve what it can of the open conflict.
    pub(in crate::repo) fn solve_with_mergiraf(&mut self, ctx: &egui::Context) {
        if let Some(ed) = self.conflict.as_mut().filter(|ed| !ed.solving) {
            ed.solving = true;
            let (repo, file) = (self.path.clone(), ed.path.clone());
            spawn(&self.tx, ctx, move || {
                let result = crate::mergiraf::solve(&repo, &file);
                Msg::Mergiraf(file, result)
            });
        }
    }

    /// Takes Mergiraf's answer into the conflict editor (if it's still on that file), and says
    /// how much it solved.
    pub(in crate::repo) fn mergiraf_done(&mut self, path: String, result: Result<String, String>) {
        let Some(ed) = self.conflict.as_mut().filter(|ed| ed.path == path) else {
            return;
        };
        ed.solving = false;
        self.status = Some(crate::repo::Notice::plain(match result {
            Ok(text) => {
                let before = ed.on_disk;
                let left = ed.take_solved(text);
                let solved = before.saturating_sub(left);
                let message = match (solved, left) {
                    (0, _) => {
                        crate::i18n::message("repo-core-0176", &[("path", path.to_string().into())])
                    }
                    (_, 0) => {
                        crate::i18n::message("repo-core-0177", &[("path", path.to_string().into())])
                    }
                    _ => crate::i18n::message(
                        "repo-core-0178",
                        &[
                            ("solved", (solved).into()),
                            ("before", (before).into()),
                            ("path", path.to_string().into()),
                            ("left", (left).into()),
                        ],
                    ),
                };
                (message, false)
            }
            Err(e) => (e, true),
        }));
    }

    /// Writes the conflict editor's file and marks it resolved.
    pub(in crate::repo) fn save_conflict(&mut self, ctx: &egui::Context) {
        // Its decisions, as they are now (see `record_decisions`).
        if self
            .conflict
            .as_ref()
            .is_some_and(|ed| !ed.saving && ed.solved().is_ok())
        {
            self.prepare_decisions();
        }
        let Some(editor) = &mut self.conflict else {
            return;
        };
        // Elsewhere, written in the background (see `conflict_saved`), one save at a time: the
        // editor waits (see `ConflictEditor::saving`) so a later one can't be overtaken.
        if crate::remote::is_remote(&self.path) {
            if editor.saving {
                return;
            }
            match editor.solved() {
                Ok(text) => {
                    editor.saving = true;
                    let (repo, path) = (self.path.clone(), editor.path.clone());
                    spawn(&self.tx, ctx, move || {
                        let saved = crate::repo::conflict::save_solved(&repo, &path, &text);
                        Msg::ConflictSaved(path, saved)
                    });
                }
                Err(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
            }
            return;
        }
        let saved = editor.save(&self.path);
        let file = editor.path.clone();
        self.conflict_saved(file, saved, ctx);
    }

    /// Marks `file` resolved once its conflicts were saved.
    pub(in crate::repo) fn conflict_saved(
        &mut self,
        file: String,
        saved: Result<(), String>,
        ctx: &egui::Context,
    ) {
        match saved {
            Ok(()) => {
                self.run_prepared(
                    "Mark resolved",
                    &crate::i18n::message("repo-core-0180", &[("file", file.to_string().into())]),
                    ops(&[&["--literal-pathspecs", "add", "--", &file]]),
                    Refresh::Status,
                    OpKind::plain(),
                    ctx,
                );
                self.record_decisions(&file, ctx);
            }
            Err(e) => {
                // Not saved: its decisions aren't kept.
                self.decisions_pending = None;
                self.status = Some(crate::repo::Notice::plain((e, true)));
            }
        }
    }

    /// Runs `ops`, the git step that follows what was just done (on another machine, in the
    /// background meanwhile). When another operation started in the meantime, it can't: the
    /// user is told (`left`: what is left undone) rather than left to think it ran.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::repo) fn run_prepared(
        &mut self,
        label: &str,
        left: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        ctx: &egui::Context,
    ) {
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-0182",
                    &[
                        ("left", (left).into()),
                        (
                            "busy",
                            crate::repo::messages::display_operation(busy).into(),
                        ),
                    ],
                ),
                true,
            )));
            return;
        }
        self.run_ops(label, ops, refresh, kind, ctx);
    }
}
