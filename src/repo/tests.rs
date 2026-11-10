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
fn a_linked_worktree_is_watched_where_git_keeps_its_data() {
    let dir = repo("linked-watch");
    let linked = dir.with_file_name(format!("gitcraft-linked-watch-wt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&linked);
    let added = git::run(
        Some(&dir),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            &linked.to_string_lossy(),
        ],
    );
    let dirs = linked_git_dirs(&linked);
    let main = linked_git_dirs(&dir);
    let canonical = std::fs::canonicalize(dir.join(".git")).ok();
    let _ = std::fs::remove_dir_all(&linked);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(added.is_ok());
    assert!(main.is_none());
    assert!(dirs.is_some());
    let (own, common) = dirs.unwrap_or_default();
    assert_eq!(Some(common.clone()), canonical);
    assert!(own.starts_with(&common) && own != common);
    let watched = [own.clone(), common.clone()];
    // Its HEAD, the shared refs: reload; objects: nothing; its index: status and submodules;
    // its files: status.
    assert_eq!(fs_change(&own.join("HEAD"), &watched), Some(FsChange::Git));
    assert_eq!(
        fs_change(&own.join("index"), &watched),
        Some(FsChange::Index)
    );
    assert_eq!(
        fs_change(&common.join("refs/heads/side"), &watched),
        Some(FsChange::Git)
    );
    assert_eq!(fs_change(&common.join("objects/ab/cd"), &watched), None);
    assert_eq!(
        fs_change(&linked.join("src/a.rs"), &watched),
        Some(FsChange::Files)
    );
    // A main worktree as before.
    assert_eq!(
        fs_change(Path::new("/r/.git/index"), &[]),
        Some(FsChange::Index)
    );
    assert_eq!(
        fs_change(Path::new("/r/.git/MERGE_HEAD"), &[]),
        Some(FsChange::Git)
    );
    assert_eq!(fs_change(Path::new("/r/.git/FETCH_HEAD"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/.git/ORIG_HEAD"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/.git/objects/x"), &[]), None);
    assert_eq!(fs_change(Path::new("/r/a.rs"), &[]), Some(FsChange::Files));
    assert_eq!(
        fs_change(Path::new("/r/.gitmodules"), &[]),
        Some(FsChange::Index)
    );
    // A repository made with `--ref-format=reftable`.
    assert_eq!(
        fs_change(Path::new("/r/.git/reftable/tables.list"), &[]),
        Some(FsChange::Git)
    );
}

#[test]
fn a_change_on_disk_reads_again_what_it_can_have_changed() {
    // The index holds the commits submodules should be at: they are read again with the status.
    assert_eq!(
        FsChange::Index.rereads(),
        [Reread::Status, Reread::Submodules]
    );
    assert_eq!(FsChange::Files.rereads(), [Reread::Status]);
    assert_eq!(FsChange::Git.rereads(), [Reread::All]);
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
fn headline_prefers_conflicts_and_errors() {
    let merge = "Merge x failed: Auto-merging a.rs
CONFLICT (content): Merge conflict in a.rs
Automatic merge failed";
    assert_eq!(
        headline(merge),
        "Merge x failed: conflicts in 1 file, resolve them in Changes"
    );
    let push = "Push failed: To origin
 ! [rejected] main
error: failed to push some refs";
    assert_eq!(
        headline(push),
        "Push failed: error: failed to push some refs"
    );
    assert_eq!(headline("Fetch: done"), "Fetch: done");
    // A hook that stopped it is the headline, not git's own error line after its output.
    let hook = "Push failed: the pre-push hook stopped it (exit code 1)
error: tests failed
error: failed to push some refs to 'origin'";
    assert_eq!(
        headline(hook),
        "Push failed: the pre-push hook stopped it (exit code 1)"
    );
    let reused = "Merge x failed: CONFLICT (content): Merge conflict in a.rs
CONFLICT (content): Merge conflict in b.rs
Resolved 'a.rs' using previous resolution.
Automatic merge failed";
    assert_eq!(
        headline(reused),
        "Merge x failed: conflicts in 2 files, 1 resolved as before: check it, resolve the rest \
         in Changes"
    );
    let all = "Merge x failed: CONFLICT (content): Merge conflict in a.rs
Resolved 'a.rs' using previous resolution.";
    assert_eq!(
        headline(all),
        "Merge x failed: 1 file resolved as before, check it in Changes"
    );
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

#[test]
fn a_pull_request_that_cannot_open_leaves_the_builds_shown() {
    use super::{Action, RepoTab, rebase::RebaseEditor};
    let dir = repo("pr-refused");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    // No forge remote: the builds view opens without asking anything.
    tab.apply(
        Action::OpenCi {
            sha: "a".repeat(40),
            run: None,
        },
        &ctx,
    );
    let shown = tab.ci_view.is_some();
    tab.rebase = Some(RebaseEditor::new("x".into(), "x".into(), Vec::new()));
    tab.apply(Action::OpenPr(7), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shown);
    assert!(tab.pr_view.is_none());
    assert!(tab.ci_view.is_some());
}

#[cfg(unix)]
#[test]
fn an_editor_command_with_arguments_is_run_with_the_path_as_one_argument() {
    use super::apply::open_in_editor;
    let dir = std::env::temp_dir().join(format!("gitcraft-editor-args-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("a b $(x).txt");
    // `cp /dev/null <path>`: the editor's arguments, then the path, which makes the file.
    let ran = open_in_editor("cp /dev/null", &file);
    let mut made = false;
    for _ in 0..100 {
        made = file.exists();
        if made {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran.is_ok(), "{ran:?}");
    assert!(made);
}

/// Waits for the operation the tab runs to end, and takes in how it ended.
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
#[test]
fn alias_authentication_text_cannot_authorize_retry_of_remaining_steps() -> Result<(), String> {
    use super::{Action, Refresh, RepoTab};
    let dir = repo("sign-in-retry");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    // A non-transport alias imitates an authentication failure.
    let push = ops(&[&[
        "-c",
        "alias.gitcraft-test-auth=!printf \"Authentication failed for 'https://fixture.invalid/team/repo'\\n\" >&2; exit 1",
        "gitcraft-test-auth",
    ]]);
    let steps: Vec<Vec<String>> = ops(&[&["tag", "v1"]])
        .into_iter()
        .chain(push.clone())
        .collect();
    tab.apply(
        Action::Ops {
            label: "Create tag v1".to_owned(),
            ops: steps,
            refresh: Refresh::All,
        },
        &ctx,
    );
    finish(&mut tab, &ctx);
    let asked = matches!(tab.dialog, Some(super::Dialog::Credentials { .. }));
    let kept = tab.last_op.as_ref().map(|op| op.ops.clone());
    let creds =
        git::Credentials::for_url("https://fixture.invalid/team/repo", "u".into(), "p".into())?;
    tab.sign_in(creds, &ctx);
    finish(&mut tab, &ctx);
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!asked);
    assert_eq!(kept, Some(push));
    assert!(tab.credentials.is_none());
    assert!(tab.busy_op.is_none() && !tab.https_checking);
    assert!(
        status.contains("destination or account context changed"),
        "{status}"
    );
    Ok(())
}

/// Aborting a merge or rebase throws away the conflicts resolved so far, with no undo: it asks
/// first.
#[test]
fn aborting_asks_first() {
    use super::{Action, Dialog, abort_action};
    let asks = match abort_action(git::InProgress::Merge) {
        Action::Dialog(Dialog::Confirm { ops: steps, .. }) => Some(steps),
        _ => None,
    };
    assert_eq!(asks, Some(ops(&[&["merge", "--abort"]])));
}

/// An undo whose alias prints authentication text remains undoable, but that
/// text cannot authorize a credential-bearing retry or move the timeline.
#[test]
fn unverified_alias_authentication_keeps_undo_without_authorizing_retry() -> Result<(), String> {
    use super::timeline::{Entry, Origin, RefState};
    use super::{RepoTab, UndoEntry};
    let dir = repo("sign-in-undo");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    // The timeline is read (there is none yet) before undo goes ahead.
    for _ in 0..2000 {
        tab.poll(&ctx);
        if tab.timeline.loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // An undo git answers like one that needs signing in.
    let entry = UndoEntry {
        id: 0,
        label: "Push".to_owned(),
        undo: ops(&[&[
            "-c",
            "alias.gitcraft-test-auth=!printf \"Authentication failed for 'https://fixture.invalid/team/repo'\\n\" >&2; exit 1",
            "gitcraft-test-auth",
        ]]),
        redo: Vec::new(),
        heads: None,
        agent: None,
    };
    let id = tab.timeline.push(Entry {
        id: 0,
        parent: None,
        time: 0,
        label: "Push".to_owned(),
        origin: Origin::Gitcraft,
        before: RefState::default(),
        after: RefState::default(),
        files: Vec::new(),
        files_total: 0,
        undo: Some(entry),
        redo_child: None,
        receipt: None,
    });
    tab.undo(&ctx);
    let running = tab.busy_op.is_some();
    finish(&mut tab, &ctx);
    let asked = matches!(tab.dialog, Some(super::Dialog::Credentials { .. }));
    // Cancelled.
    tab.dialog = None;
    let after_cancel = tab.timeline.current;
    let creds =
        git::Credentials::for_url("https://fixture.invalid/team/repo", "u".into(), "p".into())?;
    tab.sign_in(creds, &ctx);
    let retrying = tab.busy_op.is_some();
    finish(&mut tab, &ctx);
    let after_retry = tab.timeline.current;
    let entries = tab.timeline.entries.len();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(running);
    assert!(!asked);
    assert_eq!(after_cancel, Some(id));
    assert!(!retrying);
    assert!(tab.credentials.is_none());
    // No authorized retry: still there, still only once.
    assert_eq!(after_retry, Some(id));
    assert_eq!(entries, 1);
    Ok(())
}

/// Work that panics (a bug in a dependency) still ends: the tab isn't left busy for good.
#[test]
fn an_operation_that_panics_leaves_the_tab_idle() {
    use super::messages::spawn_or;
    use super::{Msg, Refresh, RepoTab, spawn};
    let dir = repo("job-panics");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.busy_op = Some("Push".to_owned());
    let failed = |e| Msg::OpDone {
        label: "Push".to_owned(),
        result: Err(e),
        refresh: Refresh::Status,
        kind: OpKind::plain(),
        quiet: false,
        stopped: 0,
    };
    spawn_or(&tab.tx, &ctx, failed, || {
        std::panic::resume_unwind(Box::new("boom"))
    });
    finish(&mut tab, &ctx);
    let op_status = tab.status.as_deref().cloned().map(|(s, _)| s);
    // Work nothing waits for says so.
    tab.status = None;
    spawn(&tab.tx, &ctx, || {
        std::panic::resume_unwind(Box::new("bang".to_owned()))
    });
    for _ in 0..400 {
        tab.poll(&ctx);
        if tab.status.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let job_status = tab.status.as_deref().cloned().map(|(s, _)| s);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.busy_op.is_none());
    assert_eq!(
        op_status.as_deref(),
        Some("Push failed: Internal error: boom")
    );
    assert_eq!(job_status.as_deref(), Some("Internal error: bang"));
}

/// A drop confirmed while another operation started meanwhile (the background fetch) says so,
/// rather than closing the dialog with nothing done and nothing said.
#[test]
fn a_drop_confirmed_while_busy_says_so() {
    use super::{Action, RepoTab, quick_edit::Edit};
    let dir = repo("drop-busy");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.busy_op = Some("Fetch".to_owned());
    tab.apply(
        Action::QuickEdit {
            edit: Edit::Drop,
            hash: "0123456789abcdef".to_owned(),
            branch_guard: crate::repo::operations::expect_symref("HEAD", "refs/heads/main"),
        },
        &ctx,
    );
    let status = tab.status.as_deref().cloned();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        status,
        Some((
            "Drop commit 01234567: Fetch is still running, try again when it's done".to_owned(),
            true
        ))
    );
}

/// Waits for the dropped patch given to the tab to be done, and takes how it went.
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
#[test]
fn dropped_patches_are_applied() {
    use super::RepoTab;
    let dir = repo("dropped patch");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let file = dir.join("notes ü.txt");
    let _ = std::fs::write(&file, "one\n");
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    assert!(git::run(Some(&dir), &["commit", "-q", "-m", "notes"]).is_ok());
    // A diff, made and then undone, to apply again.
    let _ = std::fs::write(&file, "one\ntwo\n");
    let diff = git::run(Some(&dir), &["diff"]).unwrap_or_default();
    assert!(git::run(Some(&dir), &["checkout", "--", "."]).is_ok());
    let patches = std::env::temp_dir().join(format!("gitcraft-drop é {}", std::process::id()));
    let _ = std::fs::create_dir_all(&patches);
    let diff_file = patches.join("change.DIFF");
    let _ = std::fs::write(&diff_file, &diff);
    // Something else on the status line already says it failed: no matter.
    tab.status = Some(crate::repo::Notice::plain((
        "Fetch failed: offline".to_owned(),
        true,
    )));
    let given = tab.apply_dropped_patch(diff_file.clone(), &ctx);
    // A second one while the first is applied is refused, not lost silently.
    let second = tab.apply_dropped_patch(diff_file.clone(), &ctx);
    let diff_outcome = dropped_outcome(&mut tab, &ctx);
    // Git on Windows may check files out with CRLF (core.autocrlf): compare the lines.
    let applied = std::fs::read_to_string(&file)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    // Applied twice: it doesn't apply any more.
    let again = tab.apply_dropped_patch(diff_file, &ctx);
    let again_outcome = dropped_outcome(&mut tab, &ctx);
    // A mail: a commit, made and taken back, to apply again.
    assert!(git::run(Some(&dir), &["commit", "-q", "-am", "two lines"]).is_ok());
    let mail = git::run_bytes(Some(&dir), &["format-patch", "-1", "--stdout"]).unwrap_or_default();
    assert!(git::run(Some(&dir), &["reset", "-q", "--hard", "HEAD~1"]).is_ok());
    let mail_file = patches.join("0001-two lines.patch");
    let _ = std::fs::write(&mail_file, mail);
    let _ = tab.apply_dropped_patch(mail_file, &ctx);
    let mail_outcome = dropped_outcome(&mut tab, &ctx);
    let subject = git::run(Some(&dir), &["log", "-1", "--format=%s"]).unwrap_or_default();
    // Gone before it was applied.
    let _ = tab.apply_dropped_patch(patches.join("gone.patch"), &ctx);
    let gone_outcome = tab.take_dropped_patch_outcome();
    let waits_after = tab.dropped_patch_waits();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&patches);
    assert_eq!(given, Ok(()));
    assert!(second.is_err());
    assert_eq!(diff_outcome, Some(Ok(())));
    assert_eq!(applied, "one\ntwo\n");
    assert_eq!(again, Ok(()));
    assert!(matches!(again_outcome, Some(Err(_))), "{again_outcome:?}");
    assert_eq!(mail_outcome, Some(Ok(())));
    assert_eq!(subject.trim(), "two lines");
    assert!(matches!(gone_outcome, Some(Err(e)) if e.contains("gone.patch")));
    assert!(!waits_after);
}

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
        let mut tab = super::RepoTab::new(resolved.root.clone(), &ctx);
        let wait = |tab: &mut super::RepoTab, done: &dyn Fn(&super::RepoTab) -> bool| {
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

/// Closing a tab cancels its in-flight local command, including its descendants.
#[test]
fn closing_a_tab_stops_its_background_child() -> Result<(), String> {
    let dir = repo("close-process");
    let ctx = egui::Context::default();
    let tab = super::RepoTab::new_deferred(dir.clone(), &ctx);
    let marker = dir.join("started");
    let effect = dir.join("survived");
    let descendant = format!(
        "printf started > {}; sleep 1; printf survived > {}",
        crate::remote::sh_quote(&marker.to_string_lossy()),
        crate::remote::sh_quote(&effect.to_string_lossy())
    );
    let script = format!("sh -c {} & wait", crate::remote::sh_quote(&descendant));
    let (done, result) = std::sync::mpsc::channel();
    let path = dir.clone();
    super::spawn(&tab.tx, &ctx, move || {
        let ran = git::run_shell(&path, &script).map(|_| ());
        let _ = done.send(ran);
        super::Msg::Then(Box::new(|_, _| {}))
    });
    let start = std::time::Instant::now();
    while !marker.exists() {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("child did not start".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    drop(tab);
    let ran = result
        .recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    assert_eq!(ran, Err("cancelled".into()));
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let survived = effect.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        !survived,
        "the cancelled descendant performed its delayed write"
    );
    Ok(())
}

/// Shared refs and stashes are serialized even before either worktree starts loading.
#[test]
fn linked_worktrees_share_a_mutation_key_before_the_first_load() -> Result<(), String> {
    let dir = repo("shared-mutation-key");
    let linked = dir.join("linked");
    git::run(
        Some(&dir),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            &linked.to_string_lossy(),
        ],
    )?;
    assert_eq!(
        crate::tasks::mutation_schedule_key(super::messages::mutation_key(&dir)),
        crate::tasks::mutation_schedule_key(super::messages::mutation_key(&linked))
    );
    // Ownership is refined by the same background discovery used by reloads,
    // independently of the conservative scheduling bucket.
    super::messages::discover_fetch_key(&dir);
    super::messages::discover_fetch_key(&linked);
    assert_eq!(
        super::messages::fetch_key(&dir),
        super::messages::fetch_key(&linked)
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn feature_failure_continuation_runs_when_a_queued_job_is_cancelled() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new_deferred("gitcraft-cancelled-completion".into(), &ctx);
    tab.tx.cancel.cancel();
    tab.then_or(
        &ctx,
        false,
        Err,
        || Ok::<_, String>(()),
        |tab, found, _| {
            tab.status = Some(crate::repo::Notice::plain((
                found.err().unwrap_or_else(|| "cancelled work ran".into()),
                true,
            )));
        },
    );
    let start = std::time::Instant::now();
    while tab.status.is_none() && start.elapsed() < std::time::Duration::from_secs(5) {
        tab.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        tab.status.as_deref().cloned(),
        Some(("cancelled".into(), true))
    );
    Ok(())
}

/// Repeated reload requests coalesce while the first read runs; no thread per request.
#[test]
fn rapid_reloads_finish_and_coalesce() {
    let dir = repo("rapid-reloads");
    let ctx = egui::Context::default();
    // Drive reloads without an OS watcher: its arrival starts independent status
    // and submodule reads, which share `reads` but do not measure coalescing.
    let mut tab = super::RepoTab::new_deferred(dir.clone(), &ctx);
    tab.life.shown(std::time::Instant::now());
    tab.reload(&ctx);
    let settle = |tab: &mut super::RepoTab| {
        let start = std::time::Instant::now();
        while tab.loading || tab.reload_again {
            tab.poll(&ctx);
            assert!(start.elapsed() < std::time::Duration::from_secs(20));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    settle(&mut tab);
    let before = tab.reads;
    for _ in 0..100 {
        tab.reload(&ctx);
    }
    assert_eq!(tab.reads, before + 1);
    assert!(tab.reload_again);
    settle(&mut tab);
    assert!(!tab.loading);
    assert!(!tab.reload_again);
    assert!(tab.load_error.is_none());
    assert!(tab.history.iter().any(|commit| commit.summary == "base"));
    assert_eq!(tab.reads, before + 2);
    drop(tab);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn quick_drop_refuses_a_different_branch_at_the_same_commit() -> Result<(), String> {
    use super::{Action, Msg, RepoTab, quick_edit::Edit};
    let dir = repo("quick-drop-same-sha");
    git::run(Some(&dir), &["checkout", "-qb", "original"])?;
    let base = git::head(&dir);
    git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "tip"])?;
    let tip = git::head(&dir);
    git::run(Some(&dir), &["branch", "other"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs.current = Some("original".into());
    tab.refs.head = tip.clone();
    let captured = Action::QuickEdit {
        edit: Edit::Drop,
        hash: tip.clone(),
        branch_guard: tab.same_branch(),
    };
    git::run(Some(&dir), &["checkout", "-q", "other"])?;
    tab.refs.current = Some("other".into());
    let execute = |tab: &mut RepoTab, action: Action| -> Result<bool, String> {
        tab.apply(action, &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let msg = tab.rx.recv_timeout(remaining).map_err(|e| e.to_string())?;
            let completed = match &msg {
                Msg::OpDone { result, .. } => Some(result.is_ok()),
                _ => None,
            };
            tab.handle(msg, &ctx);
            if let Some(ok) = completed {
                return Ok(ok);
            }
        }
    };
    assert!(!execute(&mut tab, captured)?);
    assert_eq!(git::head(&dir), tip);
    assert_eq!(git::resolve(&dir, "refs/heads/original")?, tip);
    assert_eq!(git::resolve(&dir, "refs/heads/other")?, tip);
    assert!(
        tab.timeline
            .entries
            .iter()
            .all(|entry| entry.undo.is_none())
    );
    assert!(git::in_progress(&dir)?.is_none());
    let fresh = Action::QuickEdit {
        edit: Edit::Drop,
        hash: tip.clone(),
        branch_guard: tab.same_branch(),
    };
    assert!(execute(&mut tab, fresh)?);
    assert_eq!(git::head(&dir), base);
    assert_eq!(git::resolve(&dir, "refs/heads/original")?, tip);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn remote_spawn_mutation_serializes_whole_recorded_operations() -> Result<(), String> {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let ctx = egui::Context::default();
    let a = super::RepoTab::new_deferred("ssh://build/main".into(), &ctx);
    let b = super::RepoTab::new_deferred("ssh://alias/linked".into(), &ctx);
    assert_ne!(a.tx.repo, b.tx.repo);
    let (release, gate) = channel::<()>();
    let (out, events) = channel();
    let record = out.clone();
    super::messages::spawn_mutation(&a.tx, &ctx, super::Msg::JobPanicked, move || {
        let _ = record.send("expect refs/stash");
        let _ = gate.recv();
        let _ = record.send("stash pop --index");
        super::Msg::Then(Box::new(|_, _| {}))
    });
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "expect refs/stash"
    );
    let unrelated = crate::tasks::Cancellation::default();
    let unrelated_out = out.clone();
    crate::tasks::pool()?
        .submit(
            unrelated.clone(),
            crate::tasks::Priority::Background,
            Some("ssh://unrelated/fixture".into()),
            Box::new(move || {
                let _ = unrelated_out.send("unrelated remote job");
            }),
        )
        .map_err(str::to_owned)?;
    let record = out.clone();
    super::messages::spawn_mutation(&b.tx, &ctx, super::Msg::JobPanicked, move || {
        let _ = record.send("stash push");
        super::Msg::Then(Box::new(|_, _| {}))
    });
    assert!(
        crate::tasks::pool()?.queued_priority_for(&b.tx.cancel)
            == Some(crate::tasks::Priority::Background)
    );
    assert!(
        crate::tasks::pool()?.queued_priority_for(&unrelated)
            == Some(crate::tasks::Priority::Background)
    );
    // An ordinary tab read remains unkeyed and runs while A holds the bucket.
    super::messages::spawn(&b.tx, &ctx, move || {
        let _ = out.send("read");
        super::Msg::Then(Box::new(|_, _| {}))
    });
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "read"
    );
    drop(release);
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash pop --index"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "unrelated remote job"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash push"
    );
    Ok(())
}

#[test]
fn successful_prefetch_settles_omitted_and_oversized_entries() {
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new_deferred("/inert/prefetch".into(), &ctx);
    let file = git::ChangedFile {
        status: 'M',
        path: "f".into(),
        old_path: None,
        added: None,
        removed: None,
    };
    tab.files_cache.reading = true;
    tab.handle(
        super::Msg::FilesPrefetched(
            vec!["kept".into(), "omitted".into(), "oversized".into()],
            Ok(vec![
                ("kept".into(), vec![file.clone()]),
                ("oversized".into(), vec![file; 5001]),
            ]),
        ),
        &ctx,
    );
    assert!(!tab.files_cache.reading);
    assert_eq!(
        tab.files_cache.get("kept").map(|files| files.len()),
        Some(1)
    );
    for hash in ["kept", "omitted", "oversized"] {
        assert!(tab.files_cache.settled(hash));
    }
    assert!(tab.files_cache.get("omitted").is_none());
    assert!(tab.files_cache.get("oversized").is_none());
}

#[test]
fn completed_read_remains_recent_for_focus_without_retaining_its_timer() {
    let ctx = egui::Context::default();
    let mut tab = super::RepoTab::new_deferred("/inert/focus-read".into(), &ctx);
    assert!(!tab.just_read());
    let started = std::time::Instant::now();
    tab.loading = true;
    tab.load_started = Some(started);
    tab.last_load_started = Some(started);
    super::messages::record_jobs(|| tab.handle(super::Msg::Loaded(Err("fixture".into())), &ctx));
    assert!(tab.load_started.is_none());
    assert!(!tab.loading);
    assert!(tab.just_read());
    tab.last_load_started = Some(started - std::time::Duration::from_secs(4));
    assert!(!tab.just_read());
    tab.loading = true;
    assert!(tab.just_read());
}

#[test]
fn startup_snapshot_cannot_authorize_merge_even_after_a_failed_load() {
    let ctx = egui::Context::default();
    for failed in [false, true] {
        let mut tab = super::RepoTab::new_deferred("/inert/startup-snapshot".into(), &ctx);
        tab.snapshot_shown = true;
        tab.refs.current = Some("cached-old-branch".into());
        tab.load_error = failed.then(|| "read failed".into());
        assert!(tab.is_busy());
        let (_, jobs) = super::messages::record_jobs(|| {
            tab.apply(
                super::merge_action(
                    super::actions::BranchRef::local("incoming"),
                    "cached-old-branch",
                ),
                &ctx,
            );
            tab.run_ops(
                "direct merge",
                super::ops(&[&["merge", "incoming"]]),
                super::Refresh::All,
                super::OpKind::plain(),
                &ctx,
            );
        });
        assert!(jobs.is_empty());
        assert!(tab.busy_op.is_none());
        // A successful fresh read advances history_gen and retires the snapshot.
        tab.history_gen = 1;
        tab.snapshot_shown = false;
        tab.load_error = None;
        assert!(!tab.is_busy());
        let (_, jobs) = super::messages::record_jobs(|| {
            tab.apply(
                super::merge_action(super::actions::BranchRef::local("incoming"), "fresh-branch"),
                &ctx,
            );
        });
        assert!(!jobs.is_empty());
        assert!(tab.busy_op.is_some());
    }
}
