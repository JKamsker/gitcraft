//! Rewriting history: editing a commit's message, and the interactive rebase.

#[path = "core_seams/rebase_execution.rs"]
mod rebase_execution;

use super::operations::{expect_ref, expect_ref_at, expect_symref};
use super::{Msg, Refresh, RepoTab, Undo, ops, rebase, short, spawn};
use crate::git;

impl RepoTab {
    /// The latest commit is amended (its message only, whatever is staged); an older one is
    /// reworded by an interactive rebase from its parent.
    pub(super) fn reword_commit(&mut self, hash: String, message: String, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        self.reword = None;
        if hash == self.refs.head {
            let (subject, body) = message.split_once("\n\n").unwrap_or((&message, ""));
            let mut amend = vec![
                "commit",
                "--amend",
                "--only",
                "--allow-empty",
                "-m",
                subject,
            ];
            if !body.is_empty() {
                amend.extend(["-m", body]);
            }
            // HEAD as the last read saw it: a commit made since (in the terminal) is not the
            // one whose message this is, and must not get it.
            let mut steps = ops(&[&amend]);
            if self.gerrit.is_some() {
                steps[0].insert(0, crate::gerrit::AMEND.to_owned());
            }
            let guarded = [self.same_branch(), expect_ref_at("HEAD", &hash)]
                .into_iter()
                .chain(steps)
                .collect();
            self.run_undoable(
                &format!("Edit the message of {}", short(&hash)),
                guarded,
                Refresh::All,
                Undo::HeadMove { mode: "--keep" },
                ctx,
            );
            return;
        }
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::rebase_commits(&path, &format!("{hash}^"));
            Msg::RewordCommits {
                hash,
                message,
                result,
            }
        });
    }

    /// Rewords the older commit `hash` with a rebase of `result`, the commits after its parent.
    pub(super) fn reword_older(
        &mut self,
        hash: String,
        message: String,
        result: Result<Vec<git::RebaseCommit>, String>,
        ctx: &egui::Context,
    ) {
        let result = result.and_then(|commits| {
            let onto = format!("{hash}^");
            let mut editor = rebase::RebaseEditor::new(onto, short(&hash).to_owned(), commits);
            let entry = editor
                .entries
                .iter_mut()
                .find(|e| e.commit.hash == hash)
                .ok_or(crate::i18n::text("repo-core-3188"))?;
            entry.step = rebase::Step::Reword;
            entry.message = message;
            if let Some(problem) = editor.problem() {
                return Err(problem);
            }
            Ok(editor)
        });
        match result {
            Ok(editor) => self.with_rebase_command(editor, ctx, move |tab, command, ctx| {
                tab.reword_ready(&hash, command, ctx);
            }),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message("repo-core-3322", &[("e", e.to_string().into())]),
                    true,
                )))
            }
        }
    }

    /// Writes `editor`'s todo list and messages where git runs (on another machine, in the
    /// background), then `then` with the rebase command and the head it starts from.
    pub(super) fn with_rebase_command(
        &mut self,
        editor: rebase::RebaseEditor,
        ctx: &egui::Context,
        then: impl FnOnce(&mut RepoTab, Result<(Vec<String>, String), String>, &egui::Context)
        + Send
        + 'static,
    ) {
        let repo = self.path.clone();
        self.here_or_later(ctx, move || Ok((editor.command(&repo)?, editor.head)), then);
    }

    /// Runs the reword of `hash` once its rebase command is ready.
    fn reword_ready(
        &mut self,
        hash: &str,
        result: Result<(Vec<String>, String), String>,
        ctx: &egui::Context,
    ) {
        match result {
            // Started while something else ran: say so rather than drop it.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-3323",
                        &[(
                            "self",
                            self.busy_op
                                .as_deref()
                                .unwrap_or_default()
                                .to_string()
                                .into(),
                        )],
                    ),
                    true,
                )));
            }
            // The todo list replaces the commits read: not one made since.
            Ok((command, head)) => self.run_undoable(
                &format!("Edit the message of {}", short(hash)),
                vec![self.same_branch(), expect_ref_at("HEAD", &head), command],
                Refresh::All,
                Undo::HeadMove { mode: "--keep" },
                ctx,
            ),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message("repo-core-3322", &[("e", e.to_string().into())]),
                    true,
                )))
            }
        }
    }

    /// A step refusing an operation unless the branch checked out is still the one this tab
    /// last read (or HEAD still detached): another branch at the same commit passes a check of
    /// HEAD's commit, and a rewrite would then rewrite that branch.
    pub(super) fn same_branch(&self) -> Vec<String> {
        let branch = self
            .refs
            .current
            .as_deref()
            .map(|b| format!("refs/heads/{b}"))
            .unwrap_or_default();
        expect_symref("HEAD", &branch)
    }

    /// Reads the commits after `onto`, for the interactive rebase editor.
    pub(super) fn interactive_rebase(&mut self, onto: String, label: String, ctx: &egui::Context) {
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            // Pinned to the commit it is now: should `onto` (a branch) move while the editor is
            // open, the commits between would be replayed without being in the todo list, and
            // so dropped.
            let onto = match git::resolve(&path, &onto) {
                Ok(commit) => commit,
                Err(e) => {
                    return Msg::RebaseCommits {
                        onto,
                        label,
                        result: Err(e),
                    };
                }
            };
            let result = git::rebase_commits(&path, &onto);
            Msg::RebaseCommits {
                onto,
                label,
                result,
            }
        });
    }

    /// Opens the interactive rebase editor on the commits `interactive_rebase` read.
    pub(super) fn open_rebase(
        &mut self,
        onto: String,
        label: String,
        result: Result<Vec<git::RebaseCommit>, String>,
    ) {
        match result {
            Ok(commits) if commits.is_empty() => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-3324",
                        &[(
                            "label",
                            crate::repo::messages::display_operation(&label).into(),
                        )],
                    ),
                    true,
                )));
            }
            Ok(commits) => {
                self.diff = None;
                self.conflict = None;
                self.file_view = None;
                let mut editor = rebase::RebaseEditor::new(onto, label, commits);
                editor.branch_guard = Some(self.same_branch());
                self.rebase = Some(editor);
            }
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message("repo-core-3325", &[("e", e.to_string().into())]),
                    true,
                )))
            }
        }
    }

    /// Explicit impact and pushed-history confirmation shared by menu and palette rebases.
    pub(super) fn ask_plain_rebase(&mut self, onto: String, label: String, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        self.plain_rebase_request = self.plain_rebase_request.wrapping_add(1);
        let requested = self.plain_rebase_request;
        let branch = self.refs.current.clone().unwrap_or_else(|| "HEAD".into());
        let head = self.refs.head.clone();
        let guard = self.same_branch();
        let upstream = self.upstream_ref(&branch);
        let repo = self.path.clone();
        self.then(
            ctx,
            move || -> Result<String, String> {
                let pinned = git::forecast_run(
                    &repo,
                    &[
                        "--no-lazy-fetch",
                        "rev-parse",
                        "--verify",
                        "--end-of-options",
                        &format!("{onto}^{{commit}}"),
                    ],
                    &[],
                    &[0],
                    &|| true,
                )?;
                let pinned = pinned.trim().to_owned();
                if !git::investigations::oid(&pinned) {
                    return Err(crate::i18n::text("repo-core-3328").into());
                }
                Ok(pinned)
            },
            move |tab, result, ctx| {
                if tab.plain_rebase_request != requested || tab.dialog.is_some() {
                    return;
                }
                let pinned = match result {
                    Ok(p) => p,
                    Err(e) => {
                        tab.status = Some(crate::repo::Notice::plain((
                            crate::i18n::message("repo-core-3329", &[("e", e.to_string().into())]),
                            true,
                        )));
                        return;
                    }
                };
                if tab.busy_op.is_some() || tab.refs.head != head || tab.same_branch() != guard {
                    tab.status = Some(crate::repo::Notice::plain((
                        crate::i18n::text("repo-core-3330").into(),
                        true,
                    )));
                    return;
                }
                let ask = git::PreviewAsk::rebase(&branch, &head, &pinned, upstream);
                let steps = vec![
                    guard,
                    expect_ref_at("HEAD", &head),
                    vec!["rebase".into(), "--end-of-options".into(), pinned],
                ];
                tab.open_dialog(
                    super::Dialog::confirm(
                        &label,
                        crate::i18n::text("repo-core-3331").into(),
                        crate::i18n::text("repo-core-3051"),
                        steps,
                        None,
                    )
                    .undo(Undo::HeadMove { mode: "--keep" })
                    .preview(
                        ask,
                        crate::i18n::message(
                            "repo-core-3332",
                            &[("branch", branch.to_string().into())],
                        ),
                    ),
                    ctx,
                );
            },
        );
    }

    pub(super) fn ask_run_rebase(&mut self, ctx: &egui::Context) {
        let guard = self.same_branch();
        let Some(editor) = self.rebase.as_mut() else {
            return;
        };
        if editor.problem().is_some() || editor.starting || self.busy_op.is_some() {
            return;
        }
        if let Some(reason) = editor.moved(&self.refs.head) {
            self.status = Some(crate::repo::Notice::plain((reason, true)));
            return;
        }
        if let Err(reason) = editor.approve(&guard) {
            self.status = Some(crate::repo::Notice::plain((reason, true)));
            return;
        }
        let id = editor.id;
        let Some(editor) = self.rebase.as_ref() else {
            return;
        };
        let branch = self.refs.current.as_deref().unwrap_or("HEAD");
        let ask = git::PreviewAsk::rebase(
            branch,
            &editor.head,
            &editor.onto,
            self.upstream_ref(branch),
        );
        let mut described = vec![format!(
            "git rebase -i --autostash {}{}{}",
            if editor.update_refs {
                "--update-refs "
            } else {
                ""
            },
            if editor.no_ff { "--no-ff " } else { "" },
            if editor.onto.is_empty() {
                "--root"
            } else {
                &editor.onto
            }
        )];
        described.extend(
            editor
                .entries
                .iter()
                .map(|e| format!("todo: {} {} {}", e.step.label(), e.commit.hash, e.message)),
        );
        self.open_dialog(
            super::Dialog::confirm(
                "Interactive rebase",
                crate::i18n::text("repo-core-3338").into(),
                crate::i18n::text("repo-core-3242"),
                Vec::new(),
                None,
            )
            .instead(super::Action::ConfirmedRebase(id), described)
            .undo(Undo::HeadMove { mode: "--keep" })
            .preview(
                ask,
                crate::i18n::message("repo-core-3339", &[("branch", branch.to_string().into())]),
            ),
            ctx,
        );
    }

    pub(super) fn confirmed_rebase(&mut self, id: u64, ctx: &egui::Context) {
        let guard = self.same_branch();
        if !self
            .rebase
            .as_ref()
            .is_some_and(|editor| editor.id == id && editor.approved(&guard, &self.refs.head))
        {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-3340").into(),
                true,
            )));
            return;
        }
        self.run_rebase(ctx);
    }
}

/// A rebase ready to run: its command, the HEAD it starts from, and the branches it moves along
/// (their commit before, the ref).
type Ready = (Vec<String>, String, Vec<(String, String)>);

/// How to undo a rebase that moved other branches along (`refs`: their commit before, the
/// ref): HEAD's branch reset to `head`, each moved back, each refusing unless still where the
/// rebase left it. Redo runs the same rebase again (`steps`), from where it started.
fn moved_refs_undo(
    branch_guard: Vec<String>,
    head: &str,
    refs: &[(String, String)],
    steps: &[Vec<String>],
) -> Undo {
    let mut undo = vec![branch_guard, expect_ref("HEAD")];
    undo.extend(refs.iter().map(|(_, name)| expect_ref(name)));
    undo.extend(ops(&[&["reset", "-q", "--keep", head]]));
    for (before, name) in refs {
        undo.extend(ops(&[&["update-ref", name, before]]));
    }
    let mut redo: Vec<Vec<String>> = refs
        .iter()
        .map(|(before, name)| expect_ref_at(name, before))
        .collect();
    redo.extend(steps.iter().cloned());
    Undo::Custom { undo, redo }
}

#[cfg(test)]
#[path = "direct_tests/rewrite_tests.rs"]
mod tests;
