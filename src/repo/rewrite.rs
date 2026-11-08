//! Rewriting history: editing a commit's message, and the interactive rebase.

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
                self.status = Some((
                    crate::i18n::message("repo-core-3322", &[("e", format!("{}", e).into())]),
                    true,
                ))
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
                self.status = Some((
                    crate::i18n::message(
                        "repo-core-3323",
                        &[(
                            "self",
                            format!("{}", self.busy_op.as_deref().unwrap_or_default()).into(),
                        )],
                    ),
                    true,
                ));
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
                self.status = Some((
                    crate::i18n::message("repo-core-3322", &[("e", format!("{}", e).into())]),
                    true,
                ))
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
                self.status = Some((
                    crate::i18n::message(
                        "repo-core-3324",
                        &[(
                            "label",
                            crate::repo::messages::display_operation(&label).into(),
                        )],
                    ),
                    true,
                ));
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
                self.status = Some((
                    crate::i18n::message("repo-core-3325", &[("e", format!("{}", e).into())]),
                    true,
                ))
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
                        tab.status = Some((
                            crate::i18n::message(
                                "repo-core-3329",
                                &[("e", format!("{}", e).into())],
                            ),
                            true,
                        ));
                        return;
                    }
                };
                if tab.busy_op.is_some() || tab.refs.head != head || tab.same_branch() != guard {
                    tab.status = Some((crate::i18n::text("repo-core-3330").into(), true));
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
                            &[("branch", format!("{}", branch).into())],
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
            self.status = Some((reason, true));
            return;
        }
        if let Err(reason) = editor.approve(&guard) {
            self.status = Some((reason, true));
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
                crate::i18n::message(
                    "repo-core-3339",
                    &[("branch", format!("{}", branch).into())],
                ),
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
            self.status = Some((crate::i18n::text("repo-core-3340").into(), true));
            return;
        }
        self.run_rebase(ctx);
    }

    /// Runs the rebase the editor describes.
    pub(super) fn run_rebase(&mut self, ctx: &egui::Context) {
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
            self.status = Some((why, true));
            return;
        }
        if editor.has_approval() {
            if !editor.approved(&guard, &self.refs.head) {
                self.status = Some((crate::i18n::text("repo-core-3341").into(), true));
                return;
            }
        } else if let Err(reason) = editor.approve(&guard) {
            self.status = Some((reason, true));
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
    fn rebase_ready(
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
            self.status = Some((crate::i18n::text("repo-core-3343").into(), true));
            return;
        }
        let Some(retained_guard) = editor.branch_guard.clone() else {
            self.status = Some((crate::i18n::text("repo-core-3344").into(), true));
            return;
        };
        match command {
            // Something else started while the files were written.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some((
                    crate::i18n::message(
                        "repo-core-3345",
                        &[(
                            "label",
                            crate::repo::messages::display_operation(&label).into(),
                        )],
                    ),
                    true,
                ));
            }
            Ok((command, head, refs)) => {
                if head != editor.head {
                    self.status = Some((crate::i18n::text("repo-core-3346").into(), true));
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
            Err(e) => self.status = Some((e, true)),
        }
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
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::super::{Msg, RepoTab, rebase};
    use crate::git;

    #[test]
    fn recording_interactive_approval_rejects_same_head_branch_and_todo_changes()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/pure-rebase-approval".into(), &ctx);
        tab.refs.current = Some("approved-A".into());
        tab.refs.head = "a".repeat(40);
        let commit = git::RebaseCommit {
            hash: tab.refs.head.clone(),
            author: "Fixture".into(),
            email: "fixture@example.invalid".into(),
            summary: "One".into(),
            message: "One".into(),
            merge: false,
        };
        let mut editor = rebase::RebaseEditor::new("b".repeat(40), "base".into(), vec![commit]);
        editor.branch_guard = Some(tab.same_branch());
        editor.approve(&tab.same_branch())?;
        let id = editor.id;
        let approved = editor.clone();
        tab.rebase = Some(editor);
        tab.refs.current = Some("unexpected-B".into());
        tab.confirmed_rebase(id, &ctx);
        assert!(tab.busy_op.is_none() && tab.last_op.is_none());
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(s, _)| s.contains("changed"))
        );
        // The same branch change during asynchronous command preparation is also
        // rejected, rather than constructing a new guard for unexpected-B.
        tab.rebase_ready(
            id,
            "Interactive rebase",
            Ok((vec!["rebase".into()], tab.refs.head.clone(), vec![])),
            &ctx,
        );
        assert!(tab.busy_op.is_none() && tab.last_op.is_none());
        tab.refs.current = Some("approved-A".into());
        let mut changed = approved;
        changed.entries[0].message = "Unapproved replacement todo".into();
        tab.rebase = Some(changed);
        tab.confirmed_rebase(id, &ctx);
        assert!(tab.busy_op.is_none() && tab.last_op.is_none());
        assert!(
            !tab.rebase
                .as_ref()
                .ok_or("missing rebase editor")?
                .approved(&tab.same_branch(), &tab.refs.head)
        );
        Ok(())
    }

    fn repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-rewrite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "root"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["commit", "-q", "--allow-empty", "-m", "read"],
        ] {
            assert!(git::run(Some(&dir), args).is_ok());
        }
        dir
    }

    /// Waits for the operation the tab started, and says how it ended.
    fn op_result(tab: &RepoTab) -> Option<Result<String, String>> {
        // Past the watcher, which comes on the same channel.
        loop {
            match tab.rx.recv_timeout(Duration::from_secs(30)) {
                Ok(Msg::OpDone { result, .. }) => return Some(result),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    #[test]
    fn plain_rebase_preview_pins_target_before_a_later_branch_move() -> Result<(), String> {
        let dir = repo("pin-plain-preview");
        let head = git::resolve(&dir, "HEAD")?;
        let target = git::resolve(&dir, "HEAD^")?;
        git::run(Some(&dir), &["branch", "target", &target])?;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.refs.head = head.clone();
        tab.refs.current = Some("main".into());
        tab.ask_plain_rebase("target".into(), "Rebase onto target".into(), &ctx);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while tab.dialog.is_none() && std::time::Instant::now() < deadline {
            if let Ok(Msg::Then(next)) = tab.rx.recv_timeout(Duration::from_millis(100)) {
                next(&mut tab, &ctx);
            }
        }
        git::run(Some(&dir), &["update-ref", "refs/heads/target", &head])?;
        let Some(super::super::Dialog::Confirm { ops, .. }) = &tab.dialog else {
            return Err("Missing rebase confirmation".into());
        };
        assert_eq!(ops.last().and_then(|s| s.last()), Some(&target));
        assert!(ops.iter().any(|s| s.iter().any(|word| word == &head)));
        tab.dialog = None;
        drop(tab);
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }

    fn subject(dir: &Path) -> String {
        git::head_message(dir)
            .map(|m| m.trim().to_owned())
            .unwrap_or_default()
    }

    /// A commit made in the terminal after the editor read the branch, and before a reload
    /// told the tab: the rebase refuses rather than drop it.
    #[test]
    fn a_rebase_does_not_drop_a_commit_the_tab_has_not_seen_yet() {
        let dir = repo("rebase-unseen");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let read = git::head(&dir);
        tab.refs.head = read.clone();
        let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
        tab.rebase = Some(rebase::RebaseEditor::new(
            "HEAD~1".to_owned(),
            "HEAD~1".to_owned(),
            commits,
        ));
        let made = git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "unseen"],
        );
        let unseen = git::head(&dir);
        tab.run_rebase(&ctx);
        let result = op_result(&tab);
        let head = git::head(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(made.is_ok());
        assert!(matches!(result, Some(Err(_))), "{result:?}");
        assert_eq!(head, unseen);
    }

    /// A rebase whose files were still being written when its editor was cancelled (or another
    /// opened) doesn't start; nor can one being started be started a second time.
    #[test]
    fn a_rebase_prepared_for_an_editor_gone_since_does_not_run() {
        let dir = repo("rebase-cancelled");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.refs.head = git::head(&dir);
        let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
        let editor = rebase::RebaseEditor::new("HEAD~1".to_owned(), "HEAD~1".to_owned(), commits);
        let id = editor.id;
        let ready = || {
            Ok((
                vec!["rebase".to_owned(), "HEAD~1".to_owned()],
                String::new(),
                Vec::new(),
            ))
        };

        // Being started: a second Start does nothing.
        tab.rebase = Some(editor.clone());
        if let Some(e) = tab.rebase.as_mut() {
            e.starting = true;
        }
        tab.run_rebase(&ctx);
        assert!(tab.busy_op.is_none());

        // Cancelled while its files were written.
        tab.rebase = None;
        tab.rebase_ready(id, "Rebase", ready(), &ctx);
        assert!(tab.busy_op.is_none());

        // Another editor opened meanwhile: that one is left alone.
        let other = rebase::RebaseEditor::new("HEAD~1".to_owned(), "HEAD~1".to_owned(), vec![]);
        let other_id = other.id;
        tab.rebase = Some(other);
        tab.rebase_ready(id, "Rebase", ready(), &ctx);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(tab.busy_op.is_none());
        assert_eq!(tab.rebase.as_ref().map(|e| e.id), Some(other_id));
        assert_ne!(id, other_id);
    }

    /// A row set to Edit (its E key) stops the rebase after that commit; what is staged then
    /// goes into it when Commit continues, and the commits after it are replayed on top.
    #[test]
    fn an_edit_step_stops_there_and_commit_amends_it() {
        let dir = repo("rebase-edit");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        tab.refs.head = git::head(&dir);
        tab.refs.current = Some("main".to_owned());
        let commits = git::rebase_commits(&dir, "HEAD~2").unwrap_or_default();
        let mut editor =
            rebase::RebaseEditor::new("HEAD~2".to_owned(), "HEAD~2".to_owned(), commits);
        editor.selected = Some(0);
        editor.press(egui::Key::E, false);
        tab.rebase = Some(editor);
        tab.run_rebase(&ctx);
        let stopped = op_result(&tab);
        let state = git::status(&dir).map(|w| w.in_progress);
        // Not staged: Continue stages it, as a commit would.
        let file = dir.join("añadido café.txt");
        let wrote = std::fs::write(&file, "edited\n");
        tab.work = git::status(&dir).unwrap_or_default();
        tab.busy_op = None;
        tab.commit(&ctx);
        let continued = op_result(&tab);
        let after = git::status(&dir).map(|w| w.in_progress);
        let base_files = git::run(
            Some(&dir),
            &[
                "-c",
                "core.quotePath=false",
                "show",
                "--name-only",
                "--format=",
                "HEAD~1",
            ],
        );
        let top = subject(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(stopped, Some(Ok(_))), "{stopped:?}");
        assert_eq!(state, Ok(Some(git::InProgress::Rebase)));
        assert!(wrote.is_ok());
        assert!(matches!(continued, Some(Ok(_))), "{continued:?}");
        assert_eq!(after, Ok(None));
        // The change went into "base", and "read" is replayed after it.
        assert!(
            base_files.is_ok_and(|f| f.contains("añadido café.txt")),
            "the edited commit holds the staged file"
        );
        assert_eq!(top, "read");
    }

    /// Esc pressed in a reword message takes the keyboard out of it, and leaves the editor
    /// open; pressed again, it closes the editor.
    #[test]
    fn esc_in_a_message_does_not_close_the_rebase_editor() {
        let dir = repo("rebase-esc");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
        tab.rebase = Some(rebase::RebaseEditor::new(
            "HEAD~1".to_owned(),
            "HEAD~1".to_owned(),
            commits,
        ));
        let esc = |tab: &mut RepoTab| {
            let input = egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            };
            let mut acts = Vec::new();
            let mut out = ctx.run_ui(input, |ui| tab.shortcuts(ui.ctx(), &mut acts));
            out.textures_delta.clear();
            acts.iter()
                .any(|a| matches!(a, super::super::Action::CloseRebase))
        };
        if let Some(e) = tab.rebase.as_mut() {
            e.typing = true;
        }
        let first = esc(&mut tab);
        if let Some(e) = tab.rebase.as_mut() {
            e.typing = false;
        }
        let second = esc(&mut tab);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!first);
        assert!(second);
    }

    /// Editing the message of what the tab thinks is the latest commit, after another was made
    /// on top: that one keeps its own message.
    #[test]
    fn rewording_the_last_commit_seen_leaves_a_newer_one_alone() {
        let dir = repo("reword-unseen");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let read = git::head(&dir);
        tab.refs.head = read.clone();
        let made = git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "unseen"],
        );
        tab.reword_commit(read, "reworded".to_owned(), &ctx);
        let result = op_result(&tab);
        let now = subject(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(made.is_ok());
        assert!(matches!(result, Some(Err(_))), "{result:?}");
        assert_eq!(now, "unseen");
    }

    /// An older commit's message: the commits are read right before, and the rebase replays
    /// them only if HEAD is still where that read saw it.
    #[test]
    fn rewording_an_older_commit_refuses_once_head_moved() {
        let dir = repo("reword-older");
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let base = git::resolve(&dir, "HEAD~1").unwrap_or_default();
        let commits = git::rebase_commits(&dir, &format!("{base}^"));
        let made = git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "unseen"],
        );
        let unseen = git::head(&dir);
        tab.reword_older(base, "reworded".to_owned(), commits, &ctx);
        let result = op_result(&tab);
        let head = git::head(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(made.is_ok());
        assert!(matches!(result, Some(Err(_))), "{result:?}");
        assert_eq!(head, unseen);
    }
}
