//! Editing a commit of the checked-out branch from its menu: its message, dropping it, moving it
//! up or down. Each is a rebase from below the commit with its todo list written here, without
//! the interactive rebase editor; undo puts the branch back.

use std::collections::HashMap;

use egui::Ui;

use super::operations::expect_ref_at;
use super::rebase::{RebaseEditor, Step};
use super::{Action, Dialog, Msg, Refresh, RepoTab, Undo, short, spawn};
use crate::git::{self, Commit, RebaseCommit};
use crate::menu_item::{MenuItem, same_reason_below};

/// A change to one commit made from its menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Message,
    Drop,
    /// Towards the newer commits, as the graph has them on top.
    MoveUp,
    MoveDown,
}

impl Edit {
    const ALL: [Edit; 4] = [Edit::Message, Edit::Drop, Edit::MoveUp, Edit::MoveDown];

    fn label(self) -> &'static str {
        match self {
            Edit::Message => crate::i18n::text("repo-core-3172"),
            Edit::Drop => crate::i18n::text("repo-core-3173"),
            Edit::MoveUp => crate::i18n::text("repo-core-3174"),
            Edit::MoveDown => crate::i18n::text("repo-core-3175"),
        }
    }

    fn operation_label(self) -> &'static str {
        match self {
            Self::Message => "Edit commit message",
            Self::Drop => "Drop commit",
            Self::MoveUp => "Move commit up",
            Self::MoveDown => "Move commit down",
        }
    }

    fn explain(self) -> &'static str {
        match self {
            Edit::Message => crate::i18n::text("repo-core-3176"),
            Edit::Drop => crate::i18n::text("repo-core-3177"),
            Edit::MoveUp => crate::i18n::text("repo-core-3178"),
            Edit::MoveDown => crate::i18n::text("repo-core-3179"),
        }
    }

    /// What the rebase replays the commits onto: below the commit, or below its parent too when
    /// it moves down past it.
    pub(super) fn base(self, hash: &str) -> String {
        match self {
            Edit::MoveDown => format!("{hash}~2"),
            _ => format!("{hash}^"),
        }
    }
}

/// The checked-out branch from HEAD down its first parents, newest first, as far as `commits`
/// are loaded.
pub(super) fn first_parents<'a, C: std::borrow::Borrow<Commit>>(
    commits: &'a [C],
    index: &HashMap<String, usize>,
    head: &str,
) -> Vec<&'a Commit> {
    let mut line = Vec::new();
    let mut next = Some(head);
    while let Some(c) = next
        .and_then(|h| index.get(h))
        .and_then(|&i| commits.get(i))
    {
        let c: &'a Commit = c.borrow();
        line.push(c);
        next = c.parents.first().map(String::as_str);
    }
    line
}

/// Why `edit` can't be done to `hash`, if it can't. `line`: the checked-out branch from HEAD
/// down its first parents.
pub(super) fn refusal(edit: Edit, line: &[&Commit], hash: &str) -> Option<&'static str> {
    let Some(pos) = line.iter().position(|c| c.hash == hash) else {
        return Some(crate::i18n::text("repo-core-3181"));
    };
    let commit = line[pos];
    // The latest commit's message is amended, which works for any commit.
    if edit == Edit::Message && pos == 0 {
        return None;
    }
    if line[..pos].iter().any(|c| c.parents.len() > 1) {
        return Some(crate::i18n::text("repo-core-2522"));
    }
    if commit.parents.len() > 1 {
        return Some(crate::i18n::text("repo-core-3182"));
    }
    if commit.parents.is_empty() {
        return Some(crate::i18n::text("repo-core-3183"));
    }
    match edit {
        Edit::MoveUp if pos == 0 => Some(crate::i18n::text("repo-core-3184")),
        Edit::MoveDown => match line.get(pos + 1) {
            None => Some(crate::i18n::text("repo-core-3185")),
            Some(below) if below.parents.len() > 1 => Some(crate::i18n::text("repo-core-3186")),
            Some(below) if below.parents.is_empty() => Some(crate::i18n::text("repo-core-3187")),
            Some(_) => None,
        },
        _ => None,
    }
}

/// The rebase that does `edit` to `hash`, from the commits after `edit.base(hash)` (oldest first,
/// as git lists them).
pub(super) fn plan(
    edit: Edit,
    hash: &str,
    commits: Vec<RebaseCommit>,
) -> Result<RebaseEditor, String> {
    if commits.iter().any(|c| c.merge) {
        return Err(crate::i18n::text("repo-core-2448").to_owned());
    }
    let mut editor = RebaseEditor::new(edit.base(hash), short(hash).to_owned(), commits);
    let i = editor
        .entries
        .iter()
        .position(|e| e.commit.hash == hash)
        .ok_or(crate::i18n::text("repo-core-3188"))?;
    match edit {
        Edit::Message => return Err(crate::i18n::text("repo-core-3189").to_owned()),
        Edit::Drop => editor.entries[i].step = Step::Drop,
        Edit::MoveUp if i + 1 < editor.entries.len() => editor.entries.swap(i, i + 1),
        Edit::MoveDown if i > 0 => editor.entries.swap(i - 1, i),
        Edit::MoveUp => return Err(crate::i18n::text("repo-core-3190").to_owned()),
        Edit::MoveDown => return Err(crate::i18n::text("repo-core-3191").to_owned()),
    }
    Ok(editor)
}

impl RepoTab {
    /// Edit message, drop, move up and down for the commit `hash`, each disabled with the
    /// reason when it can't be done.
    pub(super) fn quick_edit_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let line = first_parents(&self.history, &self.history_index, &self.refs.head);
        let blocked = if self.refs.current.is_none() {
            Some(crate::i18n::text("repo-core-0842").to_owned())
        } else {
            self.work.in_progress.map(|s| {
                crate::i18n::message(
                    "repo-core-3192",
                    &[("to-lowercase", s.label().to_lowercase().to_string().into())],
                )
            })
        };
        let refusals = Edit::ALL.map(|edit| {
            blocked
                .clone()
                .or_else(|| refusal(edit, &line, hash).map(str::to_owned))
        });
        let below = same_reason_below(&refusals);
        for ((edit, refused), below) in Edit::ALL.into_iter().zip(&refusals).zip(below) {
            let button = MenuItem::new(edit.label())
                .icon("wrench")
                .enabled(idle)
                .refused(refused.as_deref())
                .explained_below(below)
                .show(ui)
                .on_hover_text(edit.explain());
            if edit == Edit::Drop {
                super::ghost::offer(&button, || super::ghost::GhostOp::DropCommit {
                    hash: hash.to_owned(),
                });
            }
            if button.clicked() {
                acts.push(match edit {
                    Edit::Message => Action::EditMessage(hash.to_owned()),
                    Edit::Drop => self.confirm_drop(&line, hash),
                    _ => Action::QuickEdit {
                        edit,
                        hash: hash.to_owned(),
                        branch_guard: self.same_branch(),
                    },
                });
            }
        }
    }

    /// Asks before dropping `hash` (of the checked-out branch, `line` from HEAD down), saying
    /// what leaves the branch and whether it was pushed.
    fn confirm_drop(&self, line: &[&Commit], hash: &str) -> Action {
        let drop = Action::QuickEdit {
            edit: Edit::Drop,
            hash: hash.to_owned(),
            branch_guard: self.same_branch(),
        };
        let (Some(commit), Some(branch)) = (
            line.iter().find(|c| c.hash == hash),
            self.refs.current.as_deref(),
        ) else {
            return drop;
        };
        let parent = commit
            .parents
            .first()
            .map(String::as_str)
            .unwrap_or_default();
        let short = short(hash);
        let described = vec![
            format!("git rebase -i --autostash {short}^"),
            format!("todo: drop {short} {}", commit.summary),
        ];
        Action::Dialog(
            Dialog::confirm(
                "Drop commit",
                crate::i18n::message(
                    "repo-core-3196",
                    &[
                        ("short", short.to_string().into()),
                        ("summary", commit.summary.to_string().into()),
                        ("branch", branch.to_string().into()),
                    ],
                ),
                crate::i18n::text("repo-core-3197"),
                Vec::new(),
                None,
            )
            .instead(drop, described)
            .undo(Undo::HeadMove { mode: "--keep" })
            .preview(
                git::PreviewAsk::drop_commit(
                    hash,
                    parent,
                    branch,
                    &self.refs.head,
                    self.upstream_ref(branch),
                ),
                crate::i18n::message("repo-core-3198", &[("branch", branch.to_string().into())]),
            ),
        )
    }

    /// Opens the editor of the message of `hash` in the detail pane, once its detail is in.
    pub(super) fn edit_message(&mut self, hash: String, ctx: &egui::Context) {
        self.goto(&hash, ctx);
        let mut settings = crate::settings::Settings::get(ctx);
        if !settings.show_detail {
            settings.show_detail = true;
            settings.set(ctx);
        }
        self.reword = Some(super::Reword {
            hash,
            subject: String::new(),
            body: String::new(),
            focus: true,
            fill: true,
        });
        self.fill_reword();
    }

    /// Puts the message in an editor opened from the menu, when the detail it comes from is in.
    pub(super) fn fill_reword(&mut self) {
        let (Some(reword), Some(detail)) = (&mut self.reword, &self.detail) else {
            return;
        };
        if reword.fill && reword.hash == detail.hash {
            let (subject, body) = detail
                .message
                .split_once('\n')
                .map(|(s, b)| (s, b.trim()))
                .unwrap_or((detail.message.as_str(), ""));
            reword.subject = subject.to_owned();
            reword.body = body.to_owned();
            reword.fill = false;
        }
    }

    /// Reads the commits the rebase for `edit` replays.
    pub(super) fn quick_edit(
        &mut self,
        edit: Edit,
        hash: String,
        branch_guard: Vec<String>,
        ctx: &egui::Context,
    ) {
        // Confirmed (a drop) while something started meanwhile (the background fetch): said,
        // not dropped without a word.
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-3199",
                    &[
                        ("label", edit.label().to_string().into()),
                        ("hash", short(&hash).to_string().into()),
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
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::rebase_commits(&path, &edit.base(&hash));
            Msg::QuickEditCommits {
                edit,
                hash,
                branch_guard,
                result,
            }
        });
    }

    /// Runs the rebase for `edit` on the commits `quick_edit` read.
    pub(super) fn run_quick_edit(
        &mut self,
        edit: Edit,
        hash: String,
        branch_guard: Vec<String>,
        result: Result<Vec<RebaseCommit>, String>,
        ctx: &egui::Context,
    ) {
        let label = format!("{} {}", edit.operation_label(), short(&hash));
        match result.and_then(|commits| plan(edit, &hash, commits)) {
            Ok(editor) => self.with_rebase_command(editor, ctx, move |tab, command, ctx| {
                tab.quick_edit_ready(&label, branch_guard, command, ctx);
            }),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((format!("{label}: {e}"), true)))
            }
        }
    }

    /// Runs the quick edit `label` names once its rebase command is ready.
    fn quick_edit_ready(
        &mut self,
        label: &str,
        branch_guard: Vec<String>,
        command: Result<(Vec<String>, String), String>,
        ctx: &egui::Context,
    ) {
        match command {
            // Started while something else ran: say so rather than drop it.
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some(crate::repo::Notice::plain((
                    crate::i18n::message(
                        "repo-core-2452",
                        &[
                            (
                                "label",
                                crate::repo::messages::display_operation(label).into(),
                            ),
                            (
                                "self",
                                self.busy_op
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_string()
                                    .into(),
                            ),
                        ],
                    ),
                    true,
                )));
            }
            // The todo list replaces the commits read: not one made since (in the terminal).
            Ok((command, head)) => self.run_undoable(
                label,
                guarded_commands(branch_guard, &head, command),
                Refresh::All,
                Undo::HeadMove { mode: "--keep" },
                ctx,
            ),
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((format!("{label}: {e}"), true)))
            }
        }
    }
}

fn guarded_commands(branch: Vec<String>, head: &str, command: Vec<String>) -> Vec<Vec<String>> {
    vec![branch, expect_ref_at("HEAD", head), command]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_rewrites_preserve_the_original_branch_guard_before_head_and_rebase() {
        let captured = super::super::operations::expect_symref("HEAD", "refs/heads/original");
        let command = vec!["rebase".into(), "-i".into(), "base".into()];
        let operations = guarded_commands(captured.clone(), "original-head", command.clone());
        assert_eq!(
            operations,
            vec![captured, expect_ref_at("HEAD", "original-head"), command]
        );
        assert_ne!(
            operations[0],
            super::super::operations::expect_symref("HEAD", "refs/heads/other")
        );
    }

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            ..Commit::default()
        }
    }

    fn rebase_commit(hash: &str) -> RebaseCommit {
        RebaseCommit {
            hash: hash.to_owned(),
            ..RebaseCommit::default()
        }
    }

    type Planned = Result<(String, Vec<(String, Step)>), String>;

    /// What the plan for `edit` rebases onto, and its hashes and steps, oldest first.
    fn planned(edit: Edit, hash: &str, hashes: &[&str]) -> Planned {
        let commits = hashes.iter().map(|h| rebase_commit(h)).collect();
        plan(edit, hash, commits).map(|editor| {
            let steps = editor
                .entries
                .iter()
                .map(|e| (e.commit.hash.clone(), e.step))
                .collect();
            (editor.onto, steps)
        })
    }

    fn picks(hashes: &[&str]) -> Vec<(String, Step)> {
        hashes
            .iter()
            .map(|h| ((*h).to_owned(), Step::Pick))
            .collect()
    }

    #[test]
    fn dropping_marks_the_commit() {
        let mut steps = picks(&["b", "c", "d"]);
        steps[1].1 = Step::Drop;
        assert_eq!(
            planned(Edit::Drop, "c", &["b", "c", "d"]),
            Ok(("c^".to_owned(), steps))
        );
    }

    #[test]
    fn moving_swaps_with_a_neighbour() {
        assert_eq!(
            planned(Edit::MoveUp, "c", &["b", "c", "d"]),
            Ok(("c^".to_owned(), picks(&["b", "d", "c"])))
        );
        assert_eq!(
            planned(Edit::MoveDown, "c", &["b", "c", "d"]),
            Ok(("c~2".to_owned(), picks(&["c", "b", "d"])))
        );
    }

    #[test]
    fn plans_that_cant_be() {
        let commits = ["c", "d"].map(rebase_commit).to_vec();
        assert!(plan(Edit::MoveUp, "d", commits.clone()).is_err());
        assert!(plan(Edit::MoveDown, "c", commits.clone()).is_err());
        assert!(plan(Edit::Drop, "x", commits.clone()).is_err());
        let mut merged = commits;
        merged[1].merge = true;
        assert!(plan(Edit::Drop, "c", merged).is_err());
    }

    #[test]
    fn what_the_menu_allows() {
        // HEAD e, a merge d of x into c, then c, b and the root a.
        let commits = vec![
            commit("e", &["d"]),
            commit("d", &["c", "x"]),
            commit("x", &["b"]),
            commit("c", &["b"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ];
        let index = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let line = first_parents(&commits, &index, "e");
        let ok = |edit, hash| refusal(edit, &line, hash).is_none();
        assert!(ok(Edit::Message, "e"));
        assert!(ok(Edit::Drop, "e"));
        assert!(!ok(Edit::MoveUp, "e"));
        assert!(!ok(Edit::MoveDown, "e"), "the commit below is a merge");
        assert!(!ok(Edit::Drop, "d"), "a merge");
        assert!(!ok(Edit::Message, "c"), "merges after it");
        assert!(!ok(Edit::Drop, "x"), "not on the first-parent line");

        let line = first_parents(&commits, &index, "c");
        let ok = |edit, hash| refusal(edit, &line, hash).is_none();
        assert!(ok(Edit::MoveUp, "b"));
        assert!(!ok(Edit::MoveDown, "b"), "the root is below it");
        assert!(ok(Edit::MoveDown, "c"));
        assert!(!ok(Edit::Drop, "a"), "the root");
        assert!(ok(Edit::Message, "b"));
    }
}
