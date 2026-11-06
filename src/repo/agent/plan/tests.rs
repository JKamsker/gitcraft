//! Each request planned in a real repository, run as the tab runs operations, and undone the
//! way Ctrl+Z undoes it.

use std::path::{Path, PathBuf};

use super::{Plan, plan};
use crate::git;
use crate::mcp::wire::{ResetMode, WriteOp};
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, Undo, UndoEntry};

/// A repository with one commit of `a.txt`, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    write(&dir, "a.txt", "one\n");
    commit_all(&dir, "base");
    dir
}

fn write(dir: &Path, file: &str, text: &str) {
    let _ = std::fs::write(dir.join(file), text);
}

fn commit_all(dir: &Path, message: &str) {
    assert!(git::run(Some(dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(dir), &["commit", "-q", "-m", message]).is_ok());
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args).unwrap_or_default()
}

/// Runs `plan` as the tab does, and returns the undo step it leaves (if any).
fn run(dir: &Path, plan: &Plan) -> Result<Option<UndoEntry>, String> {
    let kind = OpKind::Normal(plan.undo.clone(), git::head(dir));
    let (result, kind) = run_steps(dir, &plan.ops, kind, None);
    result?;
    Ok(match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            label: plan.label.clone(),
            undo,
            redo,
            heads: None,
            agent: None,
        }),
        OpKind::Normal(Undo::HeadMove { mode }, heads) => {
            let mut parts = heads.split('\u{0}');
            let before = parts.next().unwrap_or_default().to_owned();
            Some(UndoEntry {
                label: plan.label.clone(),
                undo: crate::repo::ops(&[&["reset", mode, &before]]),
                redo: Vec::new(),
                heads: None,
                agent: None,
            })
        }
        _ => None,
    })
}

fn undo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.undo, OpKind::Undo(entry.clone()), None).0
}

/// The plan for `op`; one that fails at once (its error in the label) when it can't be made.
fn planned(dir: &Path, op: WriteOp) -> Plan {
    plan(dir, &op).unwrap_or_else(|e| Plan {
        label: format!("planning failed: {e}"),
        ops: vec![vec![format!("planning failed: {e}")]],
        refresh: crate::repo::Refresh::All,
        undo: Undo::None,
        ask: None,
    })
}

#[test]
fn undoing_a_stage_puts_back_what_was_staged_before() {
    let dir = repo("stage");
    // Partly staged already: the first change, then a second one not staged.
    write(&dir, "a.txt", "one\ntwo\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    write(&dir, "a.txt", "one\ntwo\nthree\n");
    write(&dir, "new.txt", "new\n");
    let index_before = git_out(&dir, &["ls-files", "-s"]);
    let p = planned(
        &dir,
        WriteOp::Stage {
            paths: vec!["a.txt".into(), "new.txt".into()],
        },
    );
    assert!(p.ask.is_none());
    let entry = run(&dir, &p);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let index_after = git_out(&dir, &["ls-files", "-s"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(staged, "a.txt\nnew.txt\n");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(index_after, index_before, "the index is exactly as before");
}

#[test]
fn undoing_an_unstage_stages_the_same_again() {
    let dir = repo("unstage");
    write(&dir, "a.txt", "changed\n");
    write(&dir, "b.txt", "added\n");
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    let index_before = git_out(&dir, &["ls-files", "-s"]);
    let p = planned(
        &dir,
        WriteOp::Unstage {
            paths: vec!["a.txt".into(), "b.txt".into()],
        },
    );
    let entry = run(&dir, &p);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let index_after = git_out(&dir, &["ls-files", "-s"]);
    let nothing = plan(
        &dir,
        &WriteOp::Unstage {
            paths: vec!["a.txt".into()],
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(staged, "");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(index_after, index_before);
    // Staged again by the undo: there is something to unstage once more.
    assert!(nothing.is_ok());
}

#[test]
fn a_commit_is_undone_with_its_changes_kept_staged() {
    let dir = repo("commit");
    let empty = plan(
        &dir,
        &WriteOp::Commit {
            message: "x".into(),
            amend: false,
        },
    );
    write(&dir, "a.txt", "two\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    let base = git::head(&dir);
    let p = planned(
        &dir,
        WriteOp::Commit {
            message: "Fix the login\n\nBecause.".into(),
            amend: false,
        },
    );
    let entry = run(&dir, &p);
    let message = git_out(&dir, &["log", "-1", "--format=%B"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = git::head(&dir);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(empty.is_err_and(|e| e.contains("nothing is staged")));
    // Not "Commit": that label clears the commit box the user may be writing in.
    assert_eq!(p.label, "Commit “Fix the login”");
    assert_eq!(message.trim(), "Fix the login\n\nBecause.");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(back, base);
    assert_eq!(staged, "a.txt\n");
}

#[test]
fn branches_are_made_renamed_and_checked_out_undoably() {
    let dir = repo("branches");
    let p = planned(
        &dir,
        WriteOp::CreateBranch {
            name: "feature".into(),
            start: None,
            checkout: true,
        },
    );
    let made = run(&dir, &p);
    let on = git_out(&dir, &["branch", "--show-current"]);
    let undone = made
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let gone = git::resolve(&dir, "refs/heads/feature").is_err();
    let back_on = git_out(&dir, &["branch", "--show-current"]);
    let refused = [
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "main".into(),
                start: None,
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "-f".into(),
                start: None,
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "x".into(),
                start: Some("--orphan".into()),
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::Checkout {
                branch: "main".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::Checkout {
                branch: "nope".into(),
            },
        ),
    ];
    assert!(git::run(Some(&dir), &["branch", "side"]).is_ok());
    let renamed = plan(
        &dir,
        &WriteOp::RenameBranch {
            from: "side".into(),
            to: "other".into(),
        },
    )
    .map(|p| run(&dir, &p));
    let other = git::resolve(&dir, "refs/heads/other").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(p.ask.is_none());
    assert_eq!(on.trim(), "feature");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert!(gone);
    assert_eq!(back_on.trim(), "main");
    for (i, r) in refused.iter().enumerate() {
        assert!(r.is_err(), "refusal {i}");
    }
    assert!(renamed.is_ok_and(|r| r.is_ok()));
    assert!(other);
}

#[test]
fn a_stash_is_undone_by_popping_that_stash() {
    let dir = repo("stash");
    let clean = plan(
        &dir,
        &WriteOp::Stash {
            message: "x".into(),
        },
    );
    write(&dir, "a.txt", "dirty\n");
    let p = planned(
        &dir,
        WriteOp::Stash {
            message: "wip".into(),
        },
    );
    let entry = run(&dir, &p);
    let stashed = git_out(&dir, &["status", "--porcelain"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(clean.is_err());
    assert_eq!(stashed, "");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(back, "dirty\n");
}

#[test]
fn cherry_picks_and_reverts_take_one_real_commit() {
    let dir = repo("pick");
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "side"]).is_ok());
    write(&dir, "b.txt", "b\n");
    commit_all(&dir, "add b");
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    let pick = planned(
        &dir,
        WriteOp::CherryPick {
            commit: "side".into(),
        },
    );
    let picked = run(&dir, &pick);
    let has_b = dir.join("b.txt").exists();
    let revert = planned(
        &dir,
        WriteOp::Revert {
            commit: "HEAD".into(),
        },
    );
    let reverted = run(&dir, &revert);
    let gone_b = !dir.join("b.txt").exists();
    let refused = [
        plan(
            &dir,
            &WriteOp::CherryPick {
                commit: "--abort".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::CherryPick {
                commit: "main..side".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::Revert {
                commit: "nope".into(),
            },
        ),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    assert!(pick.label.starts_with("Cherry-pick "));
    assert!(matches!(pick.undo, Undo::HeadMove { mode: "--keep" }));
    assert!(picked.is_ok() && has_b);
    assert!(reverted.is_ok() && gone_b);
    assert!(refused.iter().all(Result::is_err));
}

#[test]
fn only_a_hard_reset_asks_and_its_undo_keeps_later_work() {
    let dir = repo("reset");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let reset = |mode| {
        plan(
            &dir,
            &WriteOp::Reset {
                commit: "HEAD~1".into(),
                mode,
            },
        )
    };
    let soft = reset(ResetMode::Soft);
    let mixed = reset(ResetMode::Mixed);
    let hard = reset(ResetMode::Hard);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(soft.as_ref().is_ok_and(|p| p.ask.is_none()));
    assert!(mixed.as_ref().is_ok_and(|p| p.ask.is_none()));
    let hard = hard.ok();
    assert!(hard.as_ref().is_some_and(|p| {
        p.ask
            .as_ref()
            .is_some_and(|a| a.body.contains("can't bring those changes back"))
    }));
    assert!(hard.is_some_and(|p| matches!(p.undo, Undo::HeadMove { mode: "--keep" })));
}

#[test]
fn a_reset_refuses_once_another_branch_is_checked_out() {
    let dir = repo("reset-guard");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let p = planned(
        &dir,
        WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Mixed,
        },
    );
    // Checked out elsewhere (the terminal) before it ran.
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "other"]).is_ok());
    let ran = run(&dir, &p);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran.is_err_and(|e| e.contains("checked out now")));
}

#[test]
fn moving_a_commit_runs_and_dropping_one_asks() {
    let dir = repo("rewrite");
    write(&dir, "b.txt", "b\n");
    commit_all(&dir, "add b");
    write(&dir, "c.txt", "c\n");
    commit_all(&dir, "add c");
    let up = planned(
        &dir,
        WriteOp::MoveCommit {
            commit: "HEAD~1".into(),
            up: true,
        },
    );
    let drop = plan(
        &dir,
        &WriteOp::DropCommit {
            commit: "HEAD~1".into(),
        },
    );
    let moved = run(&dir, &up);
    let order = git_out(&dir, &["log", "--format=%s"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(up.ask.is_none());
    assert!(moved.is_ok(), "{moved:?}");
    assert_eq!(order, "add b\nadd c\nbase\n");
    let drop = drop.ok();
    assert!(
        drop.as_ref()
            .is_some_and(|p| p.ask.as_ref().is_some_and(|a| a.body.contains("add b")))
    );
    assert!(drop.is_some_and(|p| p.undo != Undo::None));
}

#[test]
fn deleting_a_branch_asks_and_can_be_undone() {
    let dir = repo("delete");
    assert!(git::run(Some(&dir), &["branch", "old"]).is_ok());
    let current = plan(
        &dir,
        &WriteOp::DeleteBranch {
            name: "main".into(),
            force: false,
        },
    );
    let p = planned(
        &dir,
        WriteOp::DeleteBranch {
            name: "old".into(),
            force: true,
        },
    );
    let entry = run(&dir, &p);
    let gone = git::resolve(&dir, "refs/heads/old").is_err();
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = git::resolve(&dir, "refs/heads/old").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(current.is_err_and(|e| e.contains("checked out")));
    assert!(p.ask.as_ref().is_some_and(|a| a.body.contains("“old”") && a.body.contains("even if it isn't merged")));
    assert!(gone);
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert!(back);
}

#[test]
fn a_push_always_asks_and_says_it_cant_be_undone() {
    let dir = repo("push");
    let nowhere = plan(&dir, &WriteOp::Push { force: false });
    let remote = dir.with_extension("remote");
    let _ = std::fs::remove_dir_all(&remote);
    assert!(git::run(None, &["init", "-q", "--bare", &remote.to_string_lossy()]).is_ok());
    assert!(
        git::run(
            Some(&dir),
            &["remote", "add", "origin", &remote.to_string_lossy()]
        )
        .is_ok()
    );
    let publish = plan(&dir, &WriteOp::Push { force: false });
    let force = plan(&dir, &WriteOp::Push { force: true });
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
    assert!(nowhere.is_err_and(|e| e.contains("no remote")));
    let publish = publish.ok();
    assert!(publish.as_ref().is_some_and(|p| p.undo == Undo::None));
    assert!(publish.as_ref().is_some_and(|p| {
        p.ask
            .as_ref()
            .is_some_and(|a| a.body.contains("Publish main on origin"))
    }));
    assert!(
        publish.is_some_and(|p| p.ops[0].contains(&"refs/heads/main:refs/heads/main".to_owned()))
    );
    assert!(force.is_ok_and(|p| p.ops[0].contains(&"--force-with-lease".to_owned())));
}

#[test]
fn staging_a_conflicted_file_asks_first() {
    let dir = repo("conflict");
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "side"]).is_ok());
    write(&dir, "a.txt", "side\n");
    commit_all(&dir, "side");
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    write(&dir, "a.txt", "main\n");
    commit_all(&dir, "main");
    let _ = git::run(Some(&dir), &["merge", "side"]);
    let stage = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec![".".into()],
        },
    );
    let unstage = plan(
        &dir,
        &WriteOp::Unstage {
            paths: vec!["a.txt".into()],
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    let stage = stage.ok();
    assert!(stage.as_ref().is_some_and(|p| p.undo == Undo::None));
    assert!(stage.is_some_and(|p| p.ask.is_some_and(|a| a.body.contains("a.txt"))));
    assert!(unstage.is_err_and(|e| e.contains("conflicted")));
}

#[test]
fn paths_from_the_request_are_checked_again() {
    let dir = repo("paths");
    let outside = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec!["../x".into()],
        },
    );
    let absolute = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec!["/etc/passwd".into()],
        },
    );
    let none = plan(&dir, &WriteOp::Stage { paths: vec![] });
    let _ = std::fs::remove_dir_all(&dir);
    assert!(outside.is_err() && absolute.is_err() && none.is_err());
}
