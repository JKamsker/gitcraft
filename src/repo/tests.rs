//! Tests of the tab's git work: undo of what operations did, the watched folders, checkout
//! names, diff answers, the toolbar's error line.

use std::path::{Path, PathBuf};

use super::commands::{checkout_remote_undo, local_name_of, refuse_option_like};
use super::operations::{run_steps, stash_push_args};
use super::toolbar::headline;
use super::watch::{FsChange, Reread, fs_change, linked_git_dirs};
use super::{DiffKey, DiffView, OpKind, Undo, ops};
use crate::git;

/// A repository with one commit, in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

#[test]
fn stashing_nothing_is_not_undoable() {
    let dir = repo("stash-nothing");
    let stash = || ops(&[&stash_push_args()]);
    let custom = || {
        OpKind::Normal(
            Undo::Custom {
                undo: ops(&[&["stash", "pop", "--index"]]),
                redo: stash(),
            },
            String::new(),
        )
    };
    // Nothing to save: undoing would pop some older stash.
    let (result, kind) = run_steps(&dir, &stash(), custom(), None);
    assert!(result.is_ok());
    assert!(matches!(kind, OpKind::Normal(Undo::None, _)));
    // Something to save: undoable.
    let _ = std::fs::write(dir.join("f.txt"), "x\n");
    let (result, kind) = run_steps(&dir, &stash(), custom(), None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok());
    assert!(matches!(kind, OpKind::Normal(Undo::Custom { .. }, _)));
}

#[test]
fn undoing_a_head_move_goes_back_to_where_head_really_was() {
    let dir = repo("head-before");
    let head = |dir: &Path| git::head(dir);
    let seen = head(&dir);
    // Committed elsewhere (the terminal) since the view last read HEAD.
    assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
    let before = head(&dir);
    let kind = OpKind::Normal(Undo::HeadMove { mode: "--soft" }, seen.clone());
    let commit = ops(&[&["commit", "-q", "--allow-empty", "-m", "three"]]);
    let (result, kind) = run_steps(&dir, &commit, kind, None);
    let after = head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(result.is_ok());
    assert_ne!(seen, before);
    assert!(matches!(
        kind,
        // Then the branch it moved, which undo checks is still the one checked out.
        OpKind::Normal(Undo::HeadMove { .. }, heads)
            if heads.starts_with(&format!("{before}\u{0}{after}\u{0}refs/heads/"))
    ));
}

#[test]
fn undoing_a_head_move_refuses_once_head_moved_elsewhere() {
    use super::UndoEntry;
    let dir = repo("undo-moved");
    let before = git::head(&dir);
    assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "two"]).is_ok());
    let after = git::head(&dir);
    let entry = UndoEntry {
        id: 0,
        label: "Commit".to_owned(),
        undo: ops(&[&["reset", "--soft", &before]]),
        redo: ops(&[&["reset", "--soft", &after]]),
        heads: Some((before.clone(), after.clone())),
        agent: None,
    };
    // Then a checkout of another branch, elsewhere: undoing must not reset that one.
    let side = [
        &["checkout", "-q", "-b", "side"][..],
        &["commit", "-q", "--allow-empty", "-m", "side work"],
    ];
    for args in side {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let side_head = git::head(&dir);
    let (refused, _) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
    let still = git::head(&dir);
    // Back where the commit left it, undoing goes through.
    let back = git::run(Some(&dir), &["checkout", "-q", "-"]);
    let (undone, kind) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
    let undone_head = git::head(&dir);
    // Redo only from where undo left it.
    let (redone, _) = run_steps(&dir, &entry.redo, OpKind::Redo(entry.clone()), None);
    let redone_head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_err());
    assert_eq!(still, side_head);
    assert!(back.is_ok());
    assert!(undone.is_ok());
    assert!(matches!(kind, OpKind::Undo(_)));
    assert_eq!(undone_head, before);
    assert!(redone.is_ok());
    assert_eq!(redone_head, after);
}

#[test]
fn an_interactive_rebase_does_not_start_once_the_branch_moved() {
    use super::{RepoTab, rebase::RebaseEditor};
    let dir = repo("rebase-moved");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let read = git::RebaseCommit {
        hash: "a".repeat(40),
        ..Default::default()
    };
    tab.rebase = Some(RebaseEditor::new("x".into(), "x".into(), vec![read]));
    // A commit made while the editor was open.
    tab.refs.head = "b".repeat(40);
    tab.run_rebase(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.busy_op.is_none());
    assert!(tab.rebase.is_some());
    assert!(tab.status.as_deref().is_some_and(|(_, err)| *err));
}

#[test]
fn undoing_a_remote_checkout_keeps_commits_made_on_the_branch_since() {
    let dir = repo("remote-checkout");
    let base = git::head(&dir);
    let setup = [
        &["remote", "add", "origin", "/nonexistent"][..],
        &["update-ref", "refs/remotes/origin/feat", &base],
        &["checkout", "-q", "-B", "main"],
    ];
    for args in setup {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let (undo, redo) = match checkout_remote_undo("main", "feat", "origin/feat") {
        Undo::Custom { undo, redo } => (undo, redo),
        _ => (Vec::new(), Vec::new()),
    };
    assert!(!undo.is_empty());
    let run = |steps: &[Vec<String>]| run_steps(&dir, steps, OpKind::plain(), None).0;
    // Nothing done on it: undoing deletes it.
    let checked_out = run(&redo);
    let undone = run(&undo);
    let gone = git::resolve(&dir, "refs/heads/feat").is_err();
    // A commit on it, made in the terminal: undoing must not delete the only branch it is on.
    let again = run(&redo);
    let work = git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "mine"]);
    let mine = git::head(&dir);
    let refused = run(&undo);
    let kept = git::resolve(&dir, "refs/heads/feat").ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(checked_out.is_ok() && undone.is_ok() && gone);
    assert!(again.is_ok() && work.is_ok());
    assert!(refused.is_err());
    assert_eq!(kept, Some(mine));
}

#[test]
fn a_remote_branch_is_checked_out_under_its_name_on_the_remote() {
    let remotes = ["origin", "my", "my/fork"];
    assert_eq!(local_name_of("origin/feature/x", &remotes), "feature/x");
    assert_eq!(local_name_of("my/fork/feat", &remotes), "feat");
    assert_eq!(local_name_of("my/feat", &remotes), "feat");
    assert_eq!(local_name_of("remotes/origin/main", &remotes), "main");
    // A remote no longer configured, whose branches are still there.
    assert_eq!(local_name_of("gone/main", &remotes), "main");
}

#[test]
fn names_git_would_read_as_options_are_not_checked_out() {
    let mut status = None;
    assert!(refuse_option_like("--orphan=x", &mut status));
    assert!(status.is_some_and(|notice| notice.1));
    let mut status = None;
    assert!(!refuse_option_like("feature/-x", &mut status));
    assert!(!refuse_option_like("refs/tags/v1", &mut status));
    assert!(status.is_none());
}

#[test]
fn an_older_diff_answer_does_not_replace_a_newer_one() {
    let key = DiffKey::Commit {
        hash: "abc".to_owned(),
        path: "a.rs".to_owned(),
    };
    let mut view = DiffView::new(key.clone());
    let older = view.next_request();
    let newer = view.next_request();
    view.arrived(newer, Err("newer".to_owned()));
    view.arrived(older, Err("older".to_owned()));
    assert!(matches!(&view.data, Some(Err(e)) if e == "newer"));
    // Nor an answer to a diff opened before, of the same file.
    let mut reopened = DiffView::new(key);
    let request = reopened.next_request();
    reopened.arrived(newer, Err("closed".to_owned()));
    assert!(reopened.data.is_none());
    reopened.arrived(request, Err("open".to_owned()));
    assert!(matches!(&reopened.data, Some(Err(e)) if e == "open"));
}

#[test]
fn an_older_read_of_the_working_tree_does_not_replace_a_newer_one() {
    use super::{Msg, RepoTab};
    let dir = repo("older-read");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let file = |path: &str| git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: '?',
        area: git::Area::Untracked,
    };
    let work = |path: &str| git::WorkStatus {
        unstaged: vec![file(path)],
        ..Default::default()
    };
    let submodule = |path: &str| git::Submodule {
        path: path.to_owned(),
        ..Default::default()
    };
    // Read 2's answers come in first, then read 1's.
    for msg in [
        Msg::Status(2, Ok(work("newer"))),
        Msg::Submodules(2, vec![submodule("newer")]),
        Msg::Status(1, Ok(work("older"))),
        Msg::Submodules(1, vec![submodule("older")]),
    ] {
        let _ = tab.tx.send(msg);
    }
    tab.poll(&ctx);
    assert_eq!(tab.work.unstaged, vec![file("newer")]);
    assert_eq!(tab.submodules.len(), 1);
    assert_eq!(tab.submodules[0].path, "newer");

    // A full reload started before them (read 1) comes in last: the working tree, and this
    // worktree's count of changes, stay those of the newer status read.
    let here = |changes| git::Worktree {
        path: dir.clone(),
        current: true,
        changes: Some(changes),
        ..Default::default()
    };
    let _ = tab.tx.send(Msg::Loaded(Ok(Box::new(super::load::Loaded {
        history_namespaces: git::history_scope::default_hidden_namespaces(),
        excluded_namespace_count: Some(0),
        refs: Default::default(),
        history: Vec::new(),
        work: git::WorkStatus::default(),
        worktrees: vec![here(0)],
        worktree_root: String::new(),
        stashes: Vec::new(),
        remotes: Vec::new(),
        forge_remotes: Vec::new(),
        forge_generation: 0,
        gerrit: None,
        submodules: Vec::new(),
        lfs: false,
        practice: None,
        identity: Default::default(),
        requested: git::LOG_PAGE,
        log_from: None,
        order: Default::default(),
        read: 1,
        issues: Default::default(),
    }))));
    tab.poll(&ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.work.unstaged, vec![file("newer")]);
    assert_eq!(tab.worktrees, vec![here(1)]);
}

#[test]
fn merging_and_rebasing_onto_a_branch_ignores_a_tag_of_the_same_name() -> Result<(), String> {
    use super::actions::{
        Action, BranchRef, fast_forward_action, merge_action, merge_message, rebase_action,
    };
    let dir = repo("tag-named-like-branch");
    let run = |args: &[&str]| git::run(Some(&dir), args);
    let rev = |r: &str| git::run(Some(&dir), &["rev-parse", r]).map(|s| s.trim().to_owned());
    // The tags `x` and `origin/x` are at the branch `x`'s older commit: `x` and `origin/x`
    // alone mean the tags to git.
    for args in [
        &["checkout", "-q", "-b", "x"][..],
        &["commit", "-q", "--allow-empty", "-m", "x1"],
        &["tag", "x"],
        &["commit", "-q", "--allow-empty", "-m", "x2"],
        &["checkout", "-q", "-b", "work", "HEAD~2"],
        &["update-ref", "refs/remotes/origin/x", "refs/heads/x"],
        &["tag", "origin/x", "x"],
    ] {
        assert!(run(args).is_ok(), "{args:?}");
    }
    let branch_tip = rev("refs/heads/x");
    let start = rev("HEAD");
    let go = |action: Action| match action {
        Action::UndoableOps { ops, .. } => run_steps(&dir, &ops, OpKind::plain(), None).0,
        _ => Err("expected undoable branch operation".to_owned()),
    };
    let ff = go(fast_forward_action(BranchRef::local("x"), "work"));
    let ff_head = rev("HEAD");
    let back = run(&["reset", "-q", "--hard", "HEAD@{1}"]);
    assert!(run(&["commit", "-q", "--allow-empty", "-m", "w1"]).is_ok());
    let merged = go(merge_action(BranchRef::local("x"), "work"));
    let merged_from = rev("HEAD^2");
    let message = run(&["log", "-1", "--format=%s"]);
    assert!(run(&["reset", "-q", "--hard", "HEAD~1"]).is_ok());
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new(dir.clone(), &ctx);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !tab.settled() || tab.history_gen == 0 {
        tab.poll(&ctx);
        assert!(
            std::time::Instant::now() < until,
            "branch tab did not settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before_rebase = rev("HEAD")?;
    let action = rebase_action(BranchRef::remote("origin/x"), "work");
    assert!(matches!(&action, Action::AskRebase { onto, .. } if onto == "refs/remotes/origin/x"));
    tab.apply(action, &ctx);
    let accepted =
        tab.accept_rebase_for_test(&ctx, "work", &before_rebase, &branch_tip.clone()?)?;
    tab.apply(accepted, &ctx);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !tab.settled() || tab.status.is_none() {
        tab.poll(&ctx);
        assert!(
            std::time::Instant::now() < until,
            "rebase did not settle: {:?}",
            tab.status
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let rebased = tab.status.as_deref().cloned().ok_or("no rebase result")?;
    let rebased_onto = rev("HEAD~1");
    drop(tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ff.is_ok(), "{ff:?}");
    assert_eq!(ff_head, branch_tip);
    assert!(back.is_ok());
    assert_ne!(start, branch_tip);
    assert!(merged.is_ok(), "{merged:?}");
    assert_eq!(merged_from, branch_tip);
    assert_eq!(
        message.map(|m| m.trim().to_owned()).as_deref(),
        Ok("Merge branch 'x' into work")
    );
    assert!(!rebased.1, "{rebased:?}");
    assert_eq!(rebased_onto, branch_tip);
    assert_eq!(
        merge_message(BranchRef::local("x"), "main"),
        "Merge branch 'x'"
    );
    assert_eq!(
        merge_message(BranchRef::remote("origin/x"), "feat"),
        "Merge remote-tracking branch 'origin/x' into feat"
    );
    Ok(())
}

fn finish(tab: &mut super::RepoTab, ctx: &egui::Context) {
    for _ in 0..400 {
        tab.poll(ctx);
        if tab.busy_op.is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Hook/alias text alone cannot authorize credentials. Completed steps remain
/// completed and the failed step stays available for an explicit manual rerun.
fn dropped_outcome(tab: &mut super::RepoTab, ctx: &egui::Context) -> Option<Result<(), String>> {
    for _ in 0..400 {
        tab.poll(ctx);
        if let Some(outcome) = tab.take_dropped_patch_outcome() {
            return Some(outcome);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    None
}

/// A patch dropped on the window goes through the same path as _Apply patch_: a bare diff onto
/// the working tree, a mail as a commit. Each tells how it went itself, whatever the tab's
/// status line says; a file that isn't there, or a patch that doesn't apply, fails.
#[path = "direct_tests/watcher_integration.rs"]
mod watcher_integration;

#[path = "direct_tests/authentication_integration.rs"]
mod authentication_integration;

#[path = "direct_tests/request_lifetimes.rs"]
mod request_lifetimes;

#[path = "direct_tests/startup_and_mutations.rs"]
mod startup_and_mutations;

/// `gitcraft blame "<file>:2"` from a subfolder, through what the window does with it: the tab
/// of the repository reads it, then shows the file's blame scrolled to and marking line 2.
#[test]
fn the_command_line_opens_a_file_s_blame_at_a_line() -> Result<(), String> {
    use std::time::{Duration, Instant};
    let dir = repo("launch blame ü");
    std::fs::create_dir_all(dir.join("sub dir")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("sub dir/a b.txt"), "one\ntwo\nthree\n").map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-q", "-m", "file"])?;
    let result = (|| {
        let cmd = crate::launch::Command::Blame {
            file: "a b.txt".into(),
            line: Some(2),
        };
        let resolved = crate::launch::resolve::resolve(&cmd, &dir.join("sub dir"))?;
        let ctx = egui::Context::default();
        let mut tab = crate::repo::RepoTab::new(resolved.root.clone(), &ctx);
        let wait = |tab: &mut crate::repo::RepoTab,
                    done: &dyn Fn(&crate::repo::RepoTab) -> bool| {
            let until = Instant::now() + Duration::from_secs(20);
            while !done(tab) && Instant::now() < until {
                tab.poll(&ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
            done(tab)
        };
        if !wait(&mut tab, &|t| !t.opening() && !t.loading) {
            return Err("the repository wasn't read".to_owned());
        }
        for action in crate::launch::actions(resolved.then) {
            tab.apply(action, &ctx);
        }
        let blamed = wait(&mut tab, &|t| {
            t.file_view
                .as_ref()
                .is_some_and(|v| matches!(v.blame, Some(Ok(_))))
        });
        let view = tab.file_view.as_ref().ok_or("no file view")?;
        Ok::<_, String>((
            blamed,
            view.path.clone(),
            view.marked_line,
            view.goto_line.get(),
        ))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let (blamed, path, marked, goto) = result?;
    assert!(blamed);
    assert_eq!(path, "sub dir/a b.txt");
    assert_eq!(marked, Some(2));
    // Asked for, and taken (scrolled to) when the blame is first drawn, which hasn't happened.
    assert_eq!(goto, Some(2));
    Ok(())
}

#[path = "direct_tests/diagnostic_presentation.rs"]
mod diagnostic_presentation;
