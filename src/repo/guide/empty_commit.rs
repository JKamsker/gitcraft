//! The empty first commit of a repository with nothing to commit: asked for once the branch
//! HEAD names is known, made only while that branch has no commits, and undone (the branch
//! deleted again) only while it is still checked out, still at that commit and not published.

use crate::git::Refs;
use crate::repo::operations::{expect_ref, expect_ref_at, expect_symref};
use crate::repo::{Dialog, RepoTab, Undo, UndoEntry, ops};

/// The message of the empty first commit.
pub(crate) const EMPTY_COMMIT_MESSAGE: &str = "Initial commit";

/// What its undo entry is called.
pub(crate) const EMPTY_COMMIT_LABEL: &str = "Empty first commit";

/// The empty first commit on `branch` (`refs/heads/main`, the unborn branch HEAD names): what
/// it runs, and how it is undone and redone.
///
/// It and its redo run only while HEAD names `branch` and has no commit, so neither goes on
/// top of a commit made since. The undo runs only while HEAD still names `branch` and the
/// branch is still at the commit made (learnt once it is made), and deletes that branch, not
/// whatever HEAD is then (a detached HEAD, or another branch at the same commit).
pub(crate) fn empty_commit_steps(branch: &str) -> (Vec<Vec<String>>, Undo) {
    let commit: Vec<Vec<String>> = [expect_ref_at("HEAD", ""), expect_symref("HEAD", branch)]
        .into_iter()
        .chain(ops(&[&[
            "commit",
            "--allow-empty",
            "-m",
            EMPTY_COMMIT_MESSAGE,
        ]]))
        .collect();
    let undo = [expect_symref("HEAD", branch), expect_ref(branch)]
        .into_iter()
        .chain(ops(&[&["update-ref", "-d", branch]]))
        .collect();
    let how = Undo::Custom {
        undo,
        redo: commit.clone(),
    };
    (commit, how)
}

/// The branch (`refs/heads/main`) an undo entry of the empty first commit deletes; `None` for
/// any other entry.
pub(crate) fn deleted_branch(entry: &UndoEntry) -> Option<&str> {
    if entry.label != EMPTY_COMMIT_LABEL {
        return None;
    }
    entry.undo.iter().find_map(|op| match op.as_slice() {
        [cmd, d, branch] if cmd == "update-ref" && d == "-d" => Some(branch.as_str()),
        _ => None,
    })
}

/// Why undoing `entry` must not run, as `refs` were read: undoing the empty first commit once
/// it is published would delete a branch the remote has, whose history then has nothing in
/// common with what is committed next.
pub(crate) fn undo_refusal(entry: &UndoEntry, refs: &Refs) -> Option<String> {
    let full = deleted_branch(entry)?;
    let name = full.strip_prefix("refs/heads/").unwrap_or(full);
    let branch = refs.local.iter().find(|b| b.name == name)?;
    let on_remote = refs
        .remotes
        .iter()
        .flat_map(|(_, list)| list)
        .any(|r| r.target == branch.target);
    (!branch.upstream.is_empty() || on_remote).then(|| {
        format!(
            "{name} is published: undoing the empty first commit would delete a branch the \
             remote has. Commit on top of it instead."
        )
    })
}

impl RepoTab {
    /// Asks before the empty first commit, once the branch HEAD names is read (in the
    /// background for a repository on another machine).
    pub(super) fn ask_empty_commit(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.here_or_later(
            ctx,
            move || {
                crate::git::run_ok_codes(Some(&repo), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
                    .map(|s| s.trim().to_owned())
            },
            |tab, branch, _| match branch {
                Ok(branch) if branch.starts_with("refs/heads/") => {
                    tab.dialog = Some(confirm_dialog(&branch));
                }
                Ok(_) => {
                    tab.status = Some((
                        "HEAD is detached: check out a branch to make the first commit on".into(),
                        true,
                    ));
                }
                Err(e) => tab.status = Some((format!("{EMPTY_COMMIT_LABEL}: {e}"), true)),
            },
        );
    }
}

/// The dialog asking before the empty first commit on `branch`.
fn confirm_dialog(branch: &str) -> Dialog {
    let (commit, undo) = empty_commit_steps(branch);
    let name = branch.strip_prefix("refs/heads/").unwrap_or(branch);
    Dialog::confirm(
        EMPTY_COMMIT_LABEL,
        format!(
            "Make an empty commit named “{EMPTY_COMMIT_MESSAGE}” on {name}? It gives the \
             repository a branch to publish and to start others from. You can undo it until \
             it is published."
        ),
        "Commit",
        commit,
        None,
    )
    .safe()
    .undo(undo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Branch;

    fn entry(label: &str) -> UndoEntry {
        let (_, undo) = empty_commit_steps("refs/heads/main");
        let Undo::Custom { undo, redo } = undo else {
            return UndoEntry {
                id: 0,
                agent: None,
                label: String::new(),
                undo: Vec::new(),
                redo: Vec::new(),
                heads: None,
            };
        };
        UndoEntry {
            id: 0,
            agent: None,
            label: label.to_owned(),
            undo,
            redo,
            heads: None,
        }
    }

    #[test]
    fn the_steps_expect_an_unborn_branch_and_undo_deletes_only_that_branch() {
        let (commit, undo) = empty_commit_steps("refs/heads/mäin");
        assert_eq!(commit.first(), Some(&expect_ref_at("HEAD", "")));
        assert_eq!(
            commit.get(1),
            Some(&expect_symref("HEAD", "refs/heads/mäin"))
        );
        assert!(matches!(undo, Undo::Custom { .. }));
        let Undo::Custom { undo, redo } = undo else {
            return;
        };
        assert_eq!(redo, commit);
        assert_eq!(
            undo.first(),
            Some(&expect_symref("HEAD", "refs/heads/mäin"))
        );
        assert_eq!(undo.get(1), Some(&expect_ref("refs/heads/mäin")));
        assert_eq!(
            undo.last(),
            ops(&[&["update-ref", "-d", "refs/heads/mäin"]]).first()
        );
    }

    #[test]
    fn only_the_empty_commit_entry_deletes_a_branch() {
        assert_eq!(
            deleted_branch(&entry(EMPTY_COMMIT_LABEL)),
            Some("refs/heads/main")
        );
        assert_eq!(deleted_branch(&entry("Commit")), None);
    }

    #[test]
    fn undo_is_refused_once_the_branch_is_published() {
        let branch = |upstream: &str| Branch {
            name: "main".into(),
            target: "a".repeat(40),
            is_head: true,
            upstream: upstream.into(),
            track: String::new(),
        };
        let local = Refs {
            local: vec![branch("")],
            ..Refs::default()
        };
        let tracked = Refs {
            local: vec![branch("origin/main")],
            ..Refs::default()
        };
        let pushed_elsewhere = Refs {
            local: vec![branch("")],
            remotes: vec![(
                "fork".into(),
                vec![Branch {
                    name: "fork/main".into(),
                    ..branch("")
                }],
            )],
            ..Refs::default()
        };
        let e = entry(EMPTY_COMMIT_LABEL);
        assert_eq!(undo_refusal(&e, &local), None);
        assert!(undo_refusal(&e, &tracked).is_some_and(|m| m.contains("main is published")));
        assert!(undo_refusal(&e, &pushed_elsewhere).is_some());
        assert_eq!(undo_refusal(&entry("Commit"), &tracked), None);
    }
}
