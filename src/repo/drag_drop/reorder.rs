//! A commit of the checked-out branch dropped on another of it: moved right above or below
//! it, or squashed into it. Like the quick edits of a commit's menu, a rebase from below the
//! older of the two with its todo list written here; undo puts the branch back.

use super::super::operations::expect_ref_at;
use super::super::rebase::{RebaseEditor, Step};
use super::super::{Refresh, RepoTab, Undo, short};
use crate::git::{self, RebaseCommit};

/// Where a dropped commit goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// Right above the target: just after it.
    Above,
    /// Right below it: just before it.
    Below,
    /// Melded into it, keeping both messages.
    Squash,
    /// Melded into it, keeping the target's message only.
    Fixup,
}

impl Place {
    pub(super) fn explain(self) -> &'static str {
        match self {
            Place::Above => crate::i18n::text("repo-core-2433"),
            Place::Below => crate::i18n::text("repo-core-2434"),
            Place::Squash => crate::i18n::text("repo-core-2435"),
            Place::Fixup => crate::i18n::text("repo-core-2436"),
        }
    }
}

/// A commit of the checked-out branch moved next to another of it, or melded into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitDrop {
    pub place: Place,
    /// The commit dropped.
    pub hash: String,
    /// The commit it was dropped on.
    pub target: String,
    pub target_summary: String,
    /// The parent of the older of the two, which the rebase replays onto.
    pub base: String,
}

impl CommitDrop {
    pub(super) fn label(&self) -> String {
        let target = format!(
            "{} “{}”",
            short(&self.target),
            cut(&self.target_summary, 36)
        );
        match self.place {
            Place::Above => crate::i18n::message(
                "repo-core-2437",
                &[
                    ("hash", short(&self.hash).to_string().into()),
                    ("target", target.to_string().into()),
                ],
            ),
            Place::Below => crate::i18n::message(
                "repo-core-2438",
                &[
                    ("hash", short(&self.hash).to_string().into()),
                    ("target", target.to_string().into()),
                ],
            ),
            Place::Squash => crate::i18n::message(
                "repo-core-2439",
                &[
                    ("hash", short(&self.hash).to_string().into()),
                    ("target", target.to_string().into()),
                ],
            ),
            Place::Fixup => crate::i18n::message(
                "repo-core-2440",
                &[
                    ("hash", short(&self.hash).to_string().into()),
                    ("target", target.to_string().into()),
                ],
            ),
        }
    }

    /// The label of the operation, for the toolbar and the undo list.
    fn op_label(&self) -> String {
        let (verb, word) = match self.place {
            Place::Above => ("Move", "above"),
            Place::Below => ("Move", "below"),
            Place::Squash => ("Squash", "into"),
            Place::Fixup => ("Fixup", "into"),
        };
        format!(
            "{verb} {} {word} {}",
            short(&self.hash),
            short(&self.target)
        )
    }
}

/// `text`, cut to `n` characters with an ellipsis.
fn cut(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_owned();
    }
    text.chars().take(n - 1).collect::<String>() + "…"
}

/// The rebase that does `drop`, from the commits after its base (oldest first, as git lists
/// them).
pub(super) fn plan(drop: &CommitDrop, commits: Vec<RebaseCommit>) -> Result<RebaseEditor, String> {
    if commits.iter().any(|c| c.merge) {
        return Err(crate::i18n::text("repo-core-2448").to_owned());
    }
    let mut editor = RebaseEditor::new(drop.base.clone(), short(&drop.base).to_owned(), commits);
    let entries = &mut editor.entries;
    let missing = crate::i18n::text("repo-core-2449");
    let from = entries
        .iter()
        .position(|e| e.commit.hash == drop.hash)
        .ok_or(missing)?;
    if !entries.iter().any(|e| e.commit.hash == drop.target) || drop.hash == drop.target {
        return Err(missing.to_owned());
    }
    let before: Vec<String> = entries.iter().map(|e| e.commit.hash.clone()).collect();
    let mut moved = entries.remove(from);
    let to = entries
        .iter()
        .position(|e| e.commit.hash == drop.target)
        .ok_or(missing)?;
    // Oldest first: above the target (newer) is after it.
    let at = match drop.place {
        Place::Below => to,
        Place::Above | Place::Squash | Place::Fixup => to + 1,
    };
    moved.step = match drop.place {
        Place::Squash => Step::Squash,
        Place::Fixup => Step::Fixup,
        Place::Above | Place::Below => Step::Pick,
    };
    entries.insert(at, moved);
    let after: Vec<&str> = entries.iter().map(|e| e.commit.hash.as_str()).collect();
    if matches!(drop.place, Place::Above | Place::Below) && after == before {
        return Err(crate::i18n::text("repo-core-2450").to_owned());
    }
    if let Some(problem) = editor.problem() {
        return Err(problem);
    }
    Ok(editor)
}

impl RepoTab {
    /// Reads the commits the rebase for `drop` replays, then runs it.
    pub(in crate::repo) fn commit_drop(&mut self, drop: CommitDrop, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let path = self.path.clone();
        let base = drop.base.clone();
        self.then(
            ctx,
            move || git::rebase_commits(&path, &base),
            move |tab, result, ctx| tab.run_commit_drop(drop, result, ctx),
        );
    }

    /// Runs the rebase for `drop` on the commits read for it.
    fn run_commit_drop(
        &mut self,
        drop: CommitDrop,
        result: Result<Vec<RebaseCommit>, String>,
        ctx: &egui::Context,
    ) {
        let label = drop.op_label();
        match result.and_then(|commits| plan(&drop, commits)) {
            Ok(editor) => self.with_rebase_command(editor, ctx, move |tab, command, ctx| {
                tab.commit_drop_ready(&label, command, ctx);
            }),
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }

    /// Runs the rebase `label` names once its command is ready: on the branch it was read
    /// from, still at the commit it was read at.
    fn commit_drop_ready(
        &mut self,
        label: &str,
        command: Result<(Vec<String>, String), String>,
        ctx: &egui::Context,
    ) {
        match command {
            Ok(_) if self.busy_op.is_some() => {
                self.status = Some((
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
                ));
            }
            Ok((command, head)) => {
                let steps = vec![self.same_branch(), expect_ref_at("HEAD", &head), command];
                self.run_undoable(
                    label,
                    steps,
                    Refresh::All,
                    Undo::HeadMove { mode: "--keep" },
                    ctx,
                );
            }
            Err(e) => self.status = Some((format!("{label}: {e}"), true)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop(place: Place, hash: &str, target: &str) -> CommitDrop {
        CommitDrop {
            place,
            hash: hash.to_owned(),
            target: target.to_owned(),
            target_summary: String::new(),
            base: "base".to_owned(),
        }
    }

    /// The hashes and steps the plan for `d` gives the commits `hashes` (oldest first).
    fn planned(d: &CommitDrop, hashes: &[&str]) -> Result<Vec<(String, Step)>, String> {
        let commits = hashes
            .iter()
            .map(|h| RebaseCommit {
                hash: (*h).to_owned(),
                ..RebaseCommit::default()
            })
            .collect();
        plan(d, commits).map(|e| {
            e.entries
                .iter()
                .map(|e| (e.commit.hash.clone(), e.step))
                .collect()
        })
    }

    fn steps(list: &[(&str, Step)]) -> Vec<(String, Step)> {
        list.iter().map(|(h, s)| ((*h).to_owned(), *s)).collect()
    }

    #[test]
    fn a_commit_moves_right_above_or_below_its_target() {
        use Step::Pick;
        let all = ["a", "b", "c", "d"];
        // `a` is the oldest: above `c` is between `c` and `d`.
        assert_eq!(
            planned(&drop(Place::Above, "a", "c"), &all),
            Ok(steps(&[("b", Pick), ("c", Pick), ("a", Pick), ("d", Pick)]))
        );
        assert_eq!(
            planned(&drop(Place::Below, "d", "b"), &all),
            Ok(steps(&[("a", Pick), ("d", Pick), ("b", Pick), ("c", Pick)]))
        );
        assert_eq!(
            planned(&drop(Place::Below, "a", "d"), &all),
            Ok(steps(&[("b", Pick), ("c", Pick), ("a", Pick), ("d", Pick)]))
        );
    }

    #[test]
    fn a_commit_is_squashed_right_after_its_target() {
        use Step::{Fixup, Pick, Squash};
        let all = ["a", "b", "c"];
        assert_eq!(
            planned(&drop(Place::Squash, "c", "a"), &all),
            Ok(steps(&[("a", Pick), ("c", Squash), ("b", Pick)]))
        );
        assert_eq!(
            planned(&drop(Place::Fixup, "a", "b"), &all),
            Ok(steps(&[("b", Pick), ("a", Fixup), ("c", Pick)]))
        );
    }

    #[test]
    fn drops_that_cant_be_planned() {
        let all = ["a", "b", "c"];
        assert!(planned(&drop(Place::Above, "a", "a"), &all).is_err());
        assert!(planned(&drop(Place::Above, "x", "a"), &all).is_err());
        assert!(planned(&drop(Place::Above, "a", "x"), &all).is_err());
        // Already where it would go.
        assert!(planned(&drop(Place::Above, "b", "a"), &all).is_err());
        assert!(planned(&drop(Place::Below, "a", "b"), &all).is_err());
        let merged = vec![
            RebaseCommit {
                hash: "a".into(),
                ..RebaseCommit::default()
            },
            RebaseCommit {
                hash: "b".into(),
                merge: true,
                ..RebaseCommit::default()
            },
        ];
        assert!(plan(&drop(Place::Squash, "b", "a"), merged).is_err());
    }

    #[test]
    fn labels_name_both_commits() {
        let mut d = drop(Place::Squash, &"1".repeat(40), &"2".repeat(40));
        d.target_summary = "A summary long enough to be cut before it ends here".into();
        assert_eq!(
            d.label(),
            "Squash 11111111 into 22222222 “A summary long enough to be cut bef…”"
        );
        assert_eq!(d.op_label(), "Squash 11111111 into 22222222");
    }
}
