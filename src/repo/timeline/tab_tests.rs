//! The timeline on real repositories, through the tab as the UI drives it: kept across a
//! restart with undo still working, changes made elsewhere seen, a rebase's old tip got back
//! after a gc, the undo tree branching and jumping, and jumps that refuse.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::view::TimelineAct;
use super::{Origin, snapshot, store};
use crate::git;
use crate::repo::actions::{BranchRef, rebase_action};
use crate::repo::{Action, RepoTab};

/// A repository with one commit, in a folder with spaces and accents in its name.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft tl ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Tëst Üser"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    // A timeline left by an earlier run of the tests.
    if let Some(file) = store::file(&dir) {
        let _ = std::fs::remove_file(file);
    }
    dir
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "{args:?}: {out:?}");
    out.unwrap_or_default().trim().to_owned()
}

/// Polls the tab until `done` holds (or ten seconds went by); says whether it did.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Settled: read, nothing running, the timeline in and not waiting for a read.
fn settled(tab: &RepoTab) -> bool {
    !tab.loading
        && tab.busy_op.is_none()
        && tab.history_gen > 0
        && tab.timeline.loaded
        && tab.timeline.awaiting == super::Awaiting::Nothing
}

fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_owned(), ctx);
    assert!(wait(&mut tab, ctx, settled), "the tab never settled");
    tab
}

/// Runs `action` as the UI would, and waits for the read after it.
fn act(tab: &mut RepoTab, ctx: &egui::Context, action: Action) {
    tab.apply(action, ctx);
    assert!(wait(tab, ctx, settled), "{:?}", tab.status);
}

fn commit(tab: &mut RepoTab, ctx: &egui::Context, file: &str, summary: &str) -> String {
    let _ = std::fs::write(tab.path.join(file), format!("{summary}\n"));
    tab.commit_summary = summary.to_owned();
    act(tab, ctx, Action::Commit);
    git::head(&tab.path)
}

fn keeper_exists(dir: &Path, id: u64) -> bool {
    git::run(
        Some(dir),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &snapshot::ref_name(&store::key(dir), id),
        ],
    )
    .is_ok()
}

/// What gitcraft did is on disk once it is done: a tab opened again on the folder has it, and
/// undoes and redoes it.
#[test]
fn operations_are_kept_across_a_restart_and_undo_still_works() {
    let dir = repo("restart");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let made = commit(&mut tab, &ctx, "dir with space ü.txt", "Add the ü file");
    let entry = tab.timeline.current_entry().cloned();
    let id = entry.as_ref().map_or(0, |e| e.id);
    // Its files and kept commits come in the background, then it is written.
    let found = wait(&mut tab, &ctx, |t| {
        t.timeline.get(id).is_some_and(|e| !e.files.is_empty())
    });
    let kept = keeper_exists(&dir, id);
    let on_disk = wait(&mut tab, &ctx, |t| {
        store::load(&t.path).is_ok_and(|s| s.entries.first().is_some_and(|e| !e.files.is_empty()))
    });
    drop(tab);

    let mut again = open(&dir, &ctx);
    let restored = again.timeline.current_entry().cloned();
    act(&mut again, &ctx, Action::Undo);
    let undone = git::head(&dir);
    let outside = again
        .timeline
        .entries
        .iter()
        .any(|e| e.origin == Origin::Outside);
    act(&mut again, &ctx, Action::Redo);
    let redone = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    let entry = entry.unwrap_or_else(|| unreachable!("no entry"));
    assert_eq!(entry.label, "Commit");
    assert_eq!(entry.origin, Origin::Gitcraft);
    assert_eq!(entry.before.head, base);
    assert_eq!(entry.after.head, made);
    assert!(entry.undo.is_some());
    assert!(found && kept && on_disk);
    let restored = restored.unwrap_or_else(|| unreachable!("nothing restored"));
    assert_eq!(restored.id, id);
    // Named by its message once read, and kept so.
    assert_eq!(restored.label, "Commit “Add the ü file”");
    assert_eq!(restored.files, ["dir with space ü.txt"]);
    assert_eq!(undone, base);
    assert!(!outside, "undoing was taken for a change made elsewhere");
    assert_eq!(redone, made);
}

/// A commit, a branch and a reset made in the terminal show up, named from the reflog; one made
/// while the tab was closed too, once it opens; and undoing one puts the branches back.
#[test]
fn changes_made_elsewhere_show_up_named_from_the_reflog() {
    let dir = repo("outside");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("p.txt"), "p\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "Fix the ü parser"]);
    tab.reload(&ctx);
    let labelled = |label: &'static str| {
        move |t: &RepoTab| t.timeline.current_entry().is_some_and(|e| e.label == label)
    };
    let committed = wait(&mut tab, &ctx, labelled("Commit “Fix the ü parser”"));
    git_ok(&dir, &["checkout", "-q", "-b", "fëature/ünï"]);
    tab.reload(&ctx);
    let checked_out = wait(&mut tab, &ctx, labelled("Checkout fëature/ünï"));
    let origins: Vec<(Origin, String)> = tab
        .timeline
        .entries
        .iter()
        .map(|e| (e.origin, e.label.clone()))
        .collect();
    drop(tab);

    // While closed.
    git_ok(&dir, &["checkout", "-q", "main"]);
    git_ok(&dir, &["reset", "-q", "--hard", "HEAD~1"]);
    let mut tab = open(&dir, &ctx);
    let while_closed = wait(&mut tab, &ctx, labelled("Reset to HEAD~1"));
    let last = tab.timeline.current_entry().cloned();
    // Undone: main goes back to the commit made in the terminal, the branch checked out too.
    act(&mut tab, &ctx, Action::Undo);
    let head_after_undo = git_ok(&dir, &["symbolic-ref", "--short", "HEAD"]);
    let main_after_undo = git_ok(&dir, &["rev-parse", "main"]);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(committed && checked_out, "{origins:?}");
    assert!(origins.iter().all(|(o, _)| *o == Origin::Outside));
    assert_eq!(origins.len(), 2);
    assert!(while_closed, "{last:?}");
    let last = last.unwrap_or_else(|| unreachable!());
    assert_eq!(last.after.head, base);
    assert_eq!(head_after_undo, "fëature/ünï");
    assert_eq!(
        main_after_undo,
        last.before
            .branches
            .get("main")
            .cloned()
            .unwrap_or_default()
    );
}

/// Yesterday's rebase, in a gitcraft opened today: its old tip is still there after the
/// reflogs expired and gc ran, and comes back as a branch, or puts the branch back.
#[test]
fn a_rebase_gives_its_old_tip_back_after_a_restart_and_a_gc() {
    let dir = repo("rebase");
    let ctx = egui::Context::default();
    git_ok(&dir, &["checkout", "-q", "-b", "feat"]);
    for n in ["f1", "f2"] {
        let _ = std::fs::write(dir.join(format!("{n}.txt")), n);
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", n]);
    }
    let old_tip = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let _ = std::fs::write(dir.join("m.txt"), "m");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "m1"]);
    git_ok(&dir, &["checkout", "-q", "feat"]);
    let mut tab = open(&dir, &ctx);
    act(
        &mut tab,
        &ctx,
        rebase_action(BranchRef::local("main"), "feat"),
    );
    let entry = tab.timeline.current_entry().cloned();
    let id = entry.as_ref().map_or(0, |e| e.id);
    let kept = wait(&mut tab, &ctx, |t| keeper_exists(&t.path, id));
    let saved = wait(&mut tab, &ctx, |t| {
        store::load(&t.path).is_ok_and(|s| s.entries.iter().any(|e| e.id == id))
    });
    drop(tab);
    // Nothing but the timeline keeps the old commits now.
    git_ok(
        &dir,
        &[
            "reflog",
            "expire",
            "--expire=now",
            "--expire-unreachable=now",
            "--all",
        ],
    );
    git_ok(&dir, &["gc", "-q", "--prune=now"]);
    let survived = git::run(
        Some(&dir),
        &["cat-file", "-e", &format!("{old_tip}^{{commit}}")],
    );
    // The graph doesn't show what is kept.
    let graph = git::log(&dir, None, 0, 100).unwrap_or_default();

    let mut tab = open(&dir, &ctx);
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Select(Some(id))), &ctx);
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::BranchAt {
            branch: "feat".to_owned(),
            commit: old_tip.clone(),
        }),
    );
    let name = format!("feat-was-{}", &old_tip[..8]);
    let branch = git::resolve(&dir, &format!("refs/heads/{name}"));
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::PutBack {
            branch: "feat".to_owned(),
            commit: old_tip.clone(),
        }),
    );
    let feat = git::head(&dir);
    let put_back = tab.timeline.current_entry().map(|e| e.label.clone());
    let _ = std::fs::remove_dir_all(&dir);

    let entry = entry.unwrap_or_else(|| unreachable!());
    assert_eq!(entry.label, "Rebase feat onto main");
    assert_eq!(entry.before.branches.get("feat"), Some(&old_tip));
    assert!(kept && saved);
    assert!(survived.is_ok(), "{survived:?}");
    assert!(
        graph
            .iter()
            .all(|c| !c.summary.starts_with("gitcraft timeline"))
    );
    assert!(graph.iter().all(|c| c.hash != old_tip));
    assert_eq!(branch, Ok(old_tip.clone()));
    assert_eq!(feat, old_tip);
    assert_eq!(
        put_back.as_deref(),
        Some(&*format!("Put feat back at {}", &old_tip[..8]))
    );
}

/// Undo three commits and commit something new: the three are a branch of the tree, a jump
/// brings back the newest of them (and undoing from there walks it), and a jump back to the new
/// commit, kept only by the timeline by then, works too.
#[test]
fn the_undo_tree_keeps_an_undone_future_to_jump_back_to() {
    let dir = repo("tree");
    let ctx = egui::Context::default();
    let base = git::head(&dir);
    let mut tab = open(&dir, &ctx);
    let c1 = commit(&mut tab, &ctx, "one.txt", "one");
    let c2 = commit(&mut tab, &ctx, "two.txt", "two");
    let c3 = commit(&mut tab, &ctx, "three.txt", "three");
    let third = tab.timeline.current;
    for _ in 0..3 {
        act(&mut tab, &ctx, Action::Undo);
    }
    let all_undone = git::head(&dir);
    let other = commit(&mut tab, &ctx, "other.txt", "other");
    let other_id = tab.timeline.current;
    let other_parent = tab.timeline.current_entry().and_then(|e| e.parent);
    let (_, lanes) = super::tree::layout(&tab.timeline);
    tab.toggle_timeline();
    act(&mut tab, &ctx, Action::Timeline(TimelineAct::Jump(third)));
    let after_jump = git::head(&dir);
    let at_third = tab.timeline.current;
    act(&mut tab, &ctx, Action::Undo);
    let after_undo = git::head(&dir);
    act(
        &mut tab,
        &ctx,
        Action::Timeline(TimelineAct::Jump(other_id)),
    );
    let back_to_other = git::head(&dir);
    let entries = tab.timeline.entries.len();
    let outside = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.origin == Origin::Outside)
        .count();
    let _ = std::fs::remove_dir_all(&dir);

    assert_ne!(c1, c2);
    assert_eq!(all_undone, base);
    assert_eq!(other_parent, None);
    assert_eq!(lanes, 2);
    assert_eq!(after_jump, c3);
    assert_eq!(at_third, third);
    assert_eq!(after_undo, c2);
    assert_eq!(back_to_other, other);
    assert_eq!(entries, 4);
    assert_eq!(outside, 0);
}

/// A jump checks the branches it moves are where they were read: one committed to since (not
/// read yet) stops it before anything runs.
#[test]
fn a_jump_refuses_once_a_branch_moved_since_it_was_read() {
    let dir = repo("jump-moved");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "a.txt", "a");
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "since"]);
    let since = git::head(&dir);
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Jump(None)), &ctx);
    let ran = wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let status = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran);
    assert!(status.contains("has moved since"), "{status}");
    assert_eq!(head, since);
}

/// Forgetting the timeline lets go of what it kept.
#[test]
fn forgetting_the_timeline_lets_go_of_its_refs() {
    let dir = repo("forget");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "a.txt", "a");
    let id = tab.timeline.current.unwrap_or_default();
    let kept = wait(&mut tab, &ctx, |t| keeper_exists(&t.path, id));
    tab.toggle_timeline();
    tab.apply(Action::Timeline(TimelineAct::Forget(0)), &ctx);
    let released = wait(&mut tab, &ctx, |t| !keeper_exists(&t.path, id));
    let empty = tab.timeline.entries.is_empty();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(kept && released && empty);
}

/// An operation started before the read after the one before it came in: both are entries, in
/// order, and Ctrl+Z waits (saying so) until the timeline knows where the repository is.
#[test]
fn back_to_back_operations_are_both_kept() {
    let dir = repo("back-to-back");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("a.txt"), "a");
    tab.commit_summary = "first".to_owned();
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none()));
    // Its read may still be on the way.
    let refused = if tab.timeline.awaiting == super::Awaiting::Nothing {
        None
    } else {
        tab.undo(&ctx);
        Some(tab.status.clone().map(|(s, _)| s).unwrap_or_default())
    };
    let _ = std::fs::write(dir.join("b.txt"), "b");
    tab.commit_summary = "second".to_owned();
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, settled));
    let labels: Vec<String> = tab
        .timeline
        .entries
        .iter()
        .map(|e| e.label.clone())
        .collect();
    let named = wait(&mut tab, &ctx, |t| {
        t.timeline.entries.iter().all(|e| e.label.contains('“'))
    });
    let path = tab.timeline.path_to(tab.timeline.current);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert!(named);
    assert_eq!(path.len(), 2);
    if let Some(status) = refused {
        assert!(status.contains("waiting for the repository"), "{status}");
    }
}

/// A commit in another worktree of the repository is that worktree's: it is no entry here, and
/// Ctrl+Z here still undoes this tab's own last operation.
#[test]
fn work_in_another_worktree_stays_off_this_timeline() {
    let dir = repo("worktrees");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let made = commit(&mut tab, &ctx, "a.txt", "mine");
    let wt = dir.with_file_name(format!(
        "{}-wt",
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    git_ok(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "other",
            &wt.to_string_lossy(),
        ],
    );
    git_ok(&wt, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    tab.reload(&ctx);
    assert!(wait(&mut tab, &ctx, |t| settled(t) && t.worktrees.len() == 2));
    let entries = tab.timeline.entries.len();
    act(&mut tab, &ctx, Action::Undo);
    let head = git::head(&dir);
    let _ = git::run(
        Some(&dir),
        &["worktree", "remove", "--force", &wt.to_string_lossy()],
    );
    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(entries, 1);
    assert_ne!(head, made);
}

/// Staging right after a commit, before its read is in, doesn't leave undo waiting for good.
#[test]
fn staging_after_an_operation_does_not_hold_undo_back() {
    use crate::repo::Refresh;
    let dir = repo("stage-after");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("a.txt"), "a");
    tab.commit_summary = "first".to_owned();
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none()));
    let _ = std::fs::write(dir.join("b.txt"), "b");
    tab.apply(
        Action::Ops {
            label: "Stage".to_owned(),
            ops: crate::repo::ops(&[&["add", "b.txt"]]),
            refresh: Refresh::Status,
        },
        &ctx,
    );
    let settles = wait(&mut tab, &ctx, settled);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(settles, "{:?}", tab.timeline.awaiting);
    assert_eq!(tab.timeline.entries.len(), 1);
}
