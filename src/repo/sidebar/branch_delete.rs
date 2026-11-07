//! Deleting a local branch, alone or with the branch it tracks on the remote.

use egui::Ui;

use crate::git::Branch;
use crate::menu_item::MenuItem;
use crate::repo::branch_cleanup::Candidate;
use crate::repo::ghost::{GhostOp, offer};
use crate::repo::operations::{expect_ref, expect_ref_at};
use crate::repo::{Action, Dialog, RepoTab, Undo, ops};

impl RepoTab {
    /// Delete…, and Delete locally and on the remote… when `b` tracks a branch that is there.
    pub(super) fn delete_items(&self, ui: &mut Ui, acts: &mut Vec<Action>, b: &Branch, idle: bool) {
        let restore = ops(&[&["branch", "--", &b.name, &b.target]]);
        // git refuses to delete a branch checked out in another worktree: with the remote's
        // deleted first (-D), that would leave its commits on no ref and nothing to undo.
        let elsewhere = self
            .worktrees
            .iter()
            .any(|w| !w.current && w.branch.as_deref() == Some(b.name.as_str()));
        let why = if elsewhere {
            "Checked out in another worktree"
        } else {
            "Can't delete the checked-out branch"
        };
        let refused = (b.is_head || elsewhere).then_some(why);
        let local_ref = format!("refs/heads/{}", b.name);
        // Only while the branch is where the menu saw it: commits made on it since (in the
        // terminal, while the dialog is open) are not what was asked to be deleted, nor what
        // undoing would bring back.
        let at_seen = || vec![expect_ref_at(&format!("refs/heads/{}", b.name), &b.target)];
        let guarded = |steps: Vec<Vec<String>>| [at_seen(), steps].concat();
        // Where the upstream is, on the remote and here.
        let upstream = (!b.upstream_gone())
            .then(|| self.split_upstream(&b.upstream))
            .flatten()
            .zip(
                self.refs
                    .remotes
                    .iter()
                    .flat_map(|(_, bs)| bs)
                    .find(|r| r.name == b.upstream),
            );
        let delete = MenuItem::new("Delete…")
            .enabled(idle)
            .refused(refused)
            // The entry below, for the remote too, says the same reason.
            .explained_below(upstream.is_some())
            .show(ui);
        offer(&delete, || GhostOp::DeleteBranch { branch: b.name.clone(), remote: false });
        if delete.clicked() {
            acts.push(Action::Dialog(
                Dialog::confirm(
                    "Delete branch",
                    format!("Delete the local branch “{}”?", b.name),
                    "Delete",
                    guarded(ops(&[&["branch", "-d", "--", &b.name]])),
                    Some((
                        "Force, even if it isn't merged (-D)".to_owned(),
                        guarded(ops(&[&["branch", "-D", "--", &b.name]])),
                    )),
                )
                .undo(Undo::Custom {
                    undo: restore.clone(),
                    redo: redo_delete(&b.name, ops(&[&["branch", "-D", "--", &b.name]])),
                })
                .preview_of(self.delete_preview(&[&b.target], vec![local_ref.clone()])),
            ));
        }
        let Some(((remote, name), tracked)) = upstream else {
            return;
        };
        let both = MenuItem::new(format!("Delete locally and on {remote}…"))
            .enabled(idle)
            .refused(refused)
            .show(ui)
            .on_hover_text(format!("Also delete {} from {remote}", b.upstream))
;
        offer(&both, || GhostOp::DeleteBranch { branch: b.name.clone(), remote: true });
        if both.clicked() {
            let remote_ref = format!("refs/heads/{name}");
            let delete =
                |flag: &str| guarded(delete_both(flag, &b.name, &remote, name, &tracked.target));
            // Undo pushes the remote branch back where it was and tracks it again.
            let undo = delete_both_undo(
                restore,
                &remote,
                &format!("{}:{remote_ref}", tracked.target),
                self.track_ops(&b.name, &b.upstream),
            );
            acts.push(Action::Dialog(
                Dialog::confirm(
                    "Delete branch locally and on the remote",
                    format!(
                        "Delete the local branch “{}” and “{name}” on {remote}? Deleting it \
                         on the remote changes it for everyone.",
                        b.name
                    ),
                    "Delete both",
                    delete("-d"),
                    Some((
                        "Force, even if it isn't merged (-D)".to_owned(),
                        delete("-D"),
                    )),
                )
                .undo(Undo::Custom {
                    undo,
                    redo: redo_delete(&b.name, delete("-D")),
                })
                // Commits pushed there that this branch hasn't go with the remote branch too.
                .preview_of(self.delete_preview(
                    &[&b.target, &tracked.target],
                    vec![local_ref, format!("refs/remotes/{}", tracked.name)],
                )),
            ));
        }
    }
}

/// Redoing the delete of `branch` with `steps` (`-D`): only while the branch is where undoing put
/// it back. Commits it got since (in the terminal) are on no other branch, and `-D` would drop
/// them along with the branch's reflog.
pub(in crate::repo) fn redo_delete(branch: &str, steps: Vec<Vec<String>>) -> Vec<Vec<String>> {
    [expect_ref(&format!("refs/heads/{branch}"))]
        .into_iter()
        .chain(steps)
        .collect()
}

/// How deleting a branch here and on `remote` is undone: `refspec` (`<commit>:refs/heads/x`)
/// pushed back first, then the branch made again with `restore` and set to track it with
/// `track`. The push is what can fail (offline, signed out): first, it leaves nothing done, and
/// undoing again later works. After the branch here, it would leave that made, and undoing again
/// would stop at making it again, never pushing back the remote one.
fn delete_both_undo(
    restore: Vec<Vec<String>>,
    remote: &str,
    refspec: &str,
    track: Vec<Vec<String>>,
) -> Vec<Vec<String>> {
    [ops(&[&["push", remote, refspec]]), restore, track].concat()
}

/// Deleting the branch `branch` (with `flag`, `-d` or `-D`) and `name` on `remote`, which was at
/// `expected` when last fetched.
fn delete_both(
    flag: &str,
    branch: &str,
    remote: &str,
    name: &str,
    expected: &str,
) -> Vec<Vec<String>> {
    let remote_ref = format!("refs/heads/{name}");
    // Only while the remote branch is where it was last fetched: commits pushed to it since (by
    // someone else) are not this branch's to throw away.
    let lease = format!("--force-with-lease={remote_ref}:{expected}");
    let here = ops(&[&["branch", flag, "--", branch]]);
    let there = ops(&[&["push", &lease, remote, "--delete", &remote_ref]]);
    // `-d` refuses before anything is gone, so it goes first. `-D` doesn't: the push, which can
    // be refused, goes first, or a refusal would leave the branch's unmerged commits deleted
    // here, with nothing to undo (a failed operation has no undo).
    if flag == "-d" {
        [here, there].concat()
    } else {
        [there, here].concat()
    }
}

/// How many branches one git command deletes or pushes back at most: a command line has a
/// length limit (32 767 characters on Windows).
const BATCH: usize = 150;

/// Deleting the branches `chosen` here, and with `on_remote` the branches they track there too,
/// as one undoable operation: `(steps, undo)`.
///
/// A failed operation has no undo, so whatever could refuse is asked before anything is
/// deleted: every branch must still be where it was offered; `git branch -f <it> <its commit>`,
/// which changes nothing, must go through (git refuses it, as it refuses `-D`, for a branch
/// checked out, rebased or bisected in any worktree); and every remote's push must go through
/// as a dry run (each remote branch where it was last fetched). Then the remote branches go,
/// one atomic push per remote, then the branches here with `-D`: the chosen ones may be
/// squash-merged or stale, which `-d` refuses. Undo pushes the remote ones back, makes the
/// branches again and has them track what they tracked.
pub(in crate::repo) fn delete_many(
    chosen: &[&Candidate],
    on_remote: bool,
) -> (Vec<Vec<String>>, Undo) {
    let guards: Vec<Vec<String>> = chosen
        .iter()
        .map(|c| expect_ref_at(&format!("refs/heads/{}", c.name), &c.target))
        .collect();
    let remotes = if on_remote {
        by_remote(chosen)
    } else {
        Vec::new()
    };
    let probes = chosen
        .iter()
        .flat_map(|c| ops(&[&["branch", "-f", "--", &c.name, &c.target]]));
    let mut dry = Vec::new();
    let mut there = Vec::new();
    let mut back = Vec::new();
    for (remote, list) in &remotes {
        for part in list.chunks(BATCH) {
            let push = push_delete(remote, part);
            let mut trial = push.clone();
            trial.insert(1, "--dry-run".to_owned());
            dry.push(trial);
            there.push(push);
            back.push(push_back(remote, part));
        }
    }
    let guards: Vec<Vec<String>> = guards.into_iter().chain(probes).chain(dry).collect();
    let names: Vec<&str> = chosen.iter().map(|c| c.name.as_str()).collect();
    let here: Vec<Vec<String>> = names
        .chunks(BATCH)
        .map(|part| {
            ["branch", "-D", "--"]
                .into_iter()
                .chain(part.iter().copied())
                .map(str::to_owned)
                .collect()
        })
        .collect();
    let steps = [guards, there, here].concat();
    let restore = chosen
        .iter()
        .flat_map(|c| ops(&[&["branch", "--", &c.name, &c.target]]));
    let track = chosen.iter().flat_map(|c| c.track.iter().cloned());
    let undo: Vec<Vec<String>> = back.into_iter().chain(restore).chain(track).collect();
    // Redone only while every branch is where undoing put it back (see `redo_delete`).
    let redo = chosen
        .iter()
        .map(|c| expect_ref(&format!("refs/heads/{}", c.name)))
        .chain(steps.iter().cloned())
        .collect();
    (steps, Undo::Custom { undo, redo })
}

/// A branch on a remote to delete: its name there and its last fetched commit.
type RemoteBranch<'a> = (&'a str, &'a str);

/// The remote branches of `chosen`, by remote, in the order first met.
fn by_remote<'a>(chosen: &[&'a Candidate]) -> Vec<(&'a str, Vec<RemoteBranch<'a>>)> {
    let mut remotes: Vec<(&str, Vec<RemoteBranch>)> = Vec::new();
    for (remote, name, at) in chosen.iter().filter_map(|c| c.remote.as_ref()) {
        let item = (name.as_str(), at.as_str());
        match remotes.iter_mut().find(|(r, _)| r == remote) {
            Some((_, list)) => list.push(item),
            None => remotes.push((remote.as_str(), vec![item])),
        }
    }
    remotes
}

/// One atomic push deleting `list` on `remote`, each only where it was last fetched.
fn push_delete(remote: &str, list: &[RemoteBranch]) -> Vec<String> {
    let leases = list
        .iter()
        .map(|(name, at)| format!("--force-with-lease=refs/heads/{name}:{at}"));
    let refs = list.iter().map(|(name, _)| format!("refs/heads/{name}"));
    ["push", "--atomic"]
        .into_iter()
        .map(str::to_owned)
        .chain(leases)
        .chain([remote.to_owned(), "--delete".to_owned()])
        .chain(refs)
        .collect()
}

/// One atomic push putting `list` back on `remote` where they were.
fn push_back(remote: &str, list: &[RemoteBranch]) -> Vec<String> {
    let specs = list
        .iter()
        .map(|(name, at)| format!("{at}:refs/heads/{name}"));
    ["push", "--atomic", remote]
        .into_iter()
        .map(str::to_owned)
        .chain(specs)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::git;

    fn run_all(dir: &Path, steps: &[Vec<String>]) -> bool {
        steps.iter().all(|op| {
            let args: Vec<&str> = op.iter().map(String::as_str).collect();
            git::run(Some(dir), &args).is_ok()
        })
    }

    /// Deleting a branch here and on the remote leaves both alone once someone pushed to the
    /// remote one since the last fetch: their commits are not ours to delete.
    #[test]
    fn remote_branch_pushed_to_since_is_not_deleted() {
        let base =
            std::env::temp_dir().join(format!("gitcraft-delete-both-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (dir, bare) = (base.join("r"), base.join("remote.git"));
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        assert!(git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()]).is_ok());
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["remote", "add", "origin", &bare.to_string_lossy()],
            &["branch", "feat"],
            &["push", "-q", "origin", "feat"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let seen = git(&["rev-parse", "refs/remotes/origin/feat"]).unwrap_or_default();
        // Someone else's commit on the remote branch, not fetched here.
        let theirs =
            git(&["commit-tree", "-p", "HEAD", "-m", "theirs", "HEAD^{tree}"]).unwrap_or_default();
        let to = format!("{}:refs/heads/feat", theirs.trim());
        assert!(git(&["push", "-q", &bare.to_string_lossy(), &to]).is_ok());
        let local = || git(&["rev-parse", "--verify", "-q", "refs/heads/feat"]).is_ok();
        // Forced: the push goes first, and is refused, so the branch here stays too.
        let forced = run_all(
            &dir,
            &delete_both("-D", "feat", "origin", "feat", seen.trim()),
        );
        let kept_forced = local();
        // Not forced: the branch here, merged, goes; the remote one stays.
        let plain = run_all(
            &dir,
            &delete_both("-d", "feat", "origin", "feat", seen.trim()),
        );
        let kept_plain = local();
        let remote = git::run(Some(&bare), &["rev-parse", "refs/heads/feat"]).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&base);
        assert!(!forced && kept_forced);
        assert!(!plain && !kept_plain);
        assert_eq!(remote.trim(), theirs.trim());
    }

    /// Undoing a delete here and on the remote that fails (the remote out of reach) can be done
    /// again later: it does not stop at making the branch here again.
    #[test]
    fn undoing_a_delete_on_the_remote_can_be_tried_again() {
        let base =
            std::env::temp_dir().join(format!("gitcraft-undo-delete-both-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (dir, bare, away) = (
            base.join("r"),
            base.join("remote.git"),
            base.join("away.git"),
        );
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        assert!(git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()]).is_ok());
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["remote", "add", "origin", &bare.to_string_lossy()],
            &["branch", "feat"],
            &["push", "-q", "origin", "feat"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let at = git::head(&dir);
        let undo = delete_both_undo(
            ops(&[&["branch", "--", "feat", &at]]),
            "origin",
            &format!("{at}:refs/heads/feat"),
            Vec::new(),
        );
        let deleted = run_all(&dir, &delete_both("-D", "feat", "origin", "feat", &at));
        // Out of reach: the undo fails…
        let moved = std::fs::rename(&bare, &away);
        let failed = !run_all(&dir, &undo);
        let back = std::fs::rename(&away, &bare);
        // …and goes through once the remote is back.
        let undone = run_all(&dir, &undo);
        let local = git::resolve(&dir, "refs/heads/feat").ok();
        let remote = git::run(Some(&bare), &["rev-parse", "refs/heads/feat"]).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&base);
        assert!(deleted && moved.is_ok() && failed && back.is_ok());
        assert!(undone);
        assert_eq!(local.as_deref(), Some(at.as_str()));
        assert_eq!(remote.trim(), at);
    }

    /// Redoing a branch's delete, once undone, leaves it alone after it got a commit since:
    /// `-D` would throw that away.
    #[test]
    fn redoing_a_delete_keeps_commits_made_on_the_branch_since() {
        use crate::repo::operations::run_steps;
        use crate::repo::{OpKind, UndoEntry};
        let dir = std::env::temp_dir().join(format!("gitcraft-redo-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let git = |args: &[&str]| git::run(Some(&dir), args);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["branch", "feat"],
        ] {
            assert!(git(args).is_ok(), "{args:?}");
        }
        let at = git::head(&dir);
        let entry = UndoEntry {
            label: "Delete branch".to_owned(),
            undo: ops(&[&["branch", "--", "feat", &at]]),
            redo: redo_delete("feat", ops(&[&["branch", "-D", "--", "feat"]])),
            heads: None,
            agent: None,
        };
        let deleted = run_all(&dir, &ops(&[&["branch", "-D", "--", "feat"]]));
        let (undone, kind) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
        // The redo, expecting the branch where undoing put it.
        let redo = match kind {
            OpKind::Undo(e) => e.redo,
            _ => Vec::new(),
        };
        // A commit on it, made elsewhere.
        let mine =
            git(&["commit-tree", "-p", "feat", "-m", "mine", "HEAD^{tree}"]).unwrap_or_default();
        let moved = git(&["update-ref", "refs/heads/feat", mine.trim()]);
        let (redone, _) = run_steps(&dir, &redo, OpKind::Redo(entry.clone()), None);
        let kept = git::resolve(&dir, "refs/heads/feat").ok();
        // Back where undo left it, redo deletes it.
        let back = git(&["update-ref", "refs/heads/feat", &at]);
        let (again, _) = run_steps(&dir, &redo, OpKind::Redo(entry), None);
        let gone = git::resolve(&dir, "refs/heads/feat").is_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(deleted && undone.is_ok() && moved.is_ok() && back.is_ok());
        assert!(!redo.is_empty());
        assert!(redone.is_err());
        assert_eq!(kept.as_deref(), Some(mine.trim()));
        assert!(again.is_ok() && gone);
    }
}
