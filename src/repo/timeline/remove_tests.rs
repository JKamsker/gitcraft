//! "Remove this, keep later work" on real repositories, through the timeline view as the UI
//! drives it: a commit in the middle taken out, a later commit that conflicts without it, the
//! last operation, and operations that can't be removed this way.

use std::path::Path;

use super::remove::Outcome;
use super::remove_tab::RemoveState;
use super::tab_tests::{act, commit, git_ok, open, repo, settled, wait};
use super::view::TimelineAct;
use crate::git;
use crate::repo::{Action, RepoTab};

/// Asks to remove entry `id` and waits for what it would do.
fn ask(tab: &mut RepoTab, ctx: &egui::Context, id: u64) -> Option<RemoveState> {
    if tab.timeline_view.is_none() {
        tab.toggle_timeline();
    }
    tab.apply(Action::Timeline(TimelineAct::AskRemove(id)), ctx);
    wait(tab, ctx, |t| {
        !matches!(
            t.timeline_view.as_ref().and_then(|v| v.remove.clone()),
            Some(RemoveState::Planning(_)) | None
        )
    });
    tab.timeline_view.as_ref().and_then(|v| v.remove.clone())
}

/// Runs the removal planned and waits for how it went.
fn run(tab: &mut RepoTab, ctx: &egui::Context) -> Option<RemoveState> {
    act(tab, ctx, Action::Timeline(TimelineAct::RunRemove));
    tab.timeline_view.as_ref().and_then(|v| v.remove.clone())
}

fn file_at(dir: &Path, rev: &str, file: &str) -> Option<String> {
    git::run(Some(dir), &["show", &format!("{rev}:{file}")]).ok()
}

/// The middle of three commits taken out: the new branch has the first and the third (played
/// again), not the second; main is as it was; undoing the removal deletes the new branch.
#[test]
fn a_commit_in_the_middle_comes_out_and_the_later_one_stays() {
    let dir = repo("remove-middle");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "one.txt", "one");
    commit(&mut tab, &ctx, "two ü.txt", "two");
    let two = tab.timeline.current.unwrap_or_default();
    let three = commit(&mut tab, &ctx, "three.txt", "three");
    let planned = ask(&mut tab, &ctx, two);
    let Some(RemoveState::Planned { plan, name }) = planned else {
        unreachable!("{planned:?}");
    };
    tab.apply(
        Action::Timeline(TimelineAct::RemoveName(format!("{name}-ü"))),
        &ctx,
    );
    let outcome = run(&mut tab, &ctx);
    let branch = format!("{name}-ü");
    let tip = git::resolve(&dir, &format!("refs/heads/{branch}")).unwrap_or_default();
    let has = |f: &str| file_at(&dir, &tip, f).is_some();
    let (one, gone, kept) = (has("one.txt"), has("two ü.txt"), has("three.txt"));
    let main = git::head(&dir);
    let label = tab.timeline.current_entry().map(|e| e.label.clone());
    let worktrees = git_ok(&dir, &["worktree", "list"]).lines().count();
    act(&mut tab, &ctx, Action::Undo);
    let after_undo = git::resolve(&dir, &format!("refs/heads/{branch}"));
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(plan.branch, "main");
    assert_eq!(plan.removed.len(), 1);
    assert_eq!(plan.removed[0].1, "two");
    assert_eq!(plan.later, std::slice::from_ref(&three));
    assert!(
        matches!(
            outcome,
            Some(RemoveState::Finished {
                outcome: Outcome::Done { .. },
                ..
            })
        ),
        "{outcome:?}"
    );
    assert!(one && !gone && kept);
    assert_eq!(main, three);
    assert_eq!(worktrees, 1, "the worktree it played in is left");
    assert!(label.is_some_and(|l| l.starts_with("Remove “Commit “two”” from main, as ")));
    assert!(after_undo.is_err());
}

/// A later commit that changed what the removed one did doesn't apply: it stops there, the
/// conflicted file named, its worktree kept with the cherry-pick under way; discarding removes
/// both.
#[test]
fn a_later_commit_that_conflicts_is_shown_and_can_be_discarded() {
    let dir = repo("remove-conflict");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "f.txt", "a");
    commit(&mut tab, &ctx, "f.txt", "b");
    let middle = tab.timeline.current.unwrap_or_default();
    commit(&mut tab, &ctx, "f.txt", "c");
    commit(&mut tab, &ctx, "g.txt", "d");
    let _ = ask(&mut tab, &ctx, middle);
    let outcome = run(&mut tab, &ctx);
    let Some(RemoveState::Finished {
        outcome:
            Outcome::Conflict {
                branch,
                folder,
                files,
                summary,
                left,
                ..
            },
        ..
    }) = outcome
    else {
        unreachable!("{outcome:?}");
    };
    let picking = git::run(
        Some(&folder),
        &["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
    );
    tab.apply(Action::Timeline(TimelineAct::AskDiscard(true)), &ctx);
    act(&mut tab, &ctx, Action::Timeline(TimelineAct::DiscardRemove));
    let folder_left = folder.exists();
    let branch_left = git::resolve(&dir, &format!("refs/heads/{branch}"));
    let _ = std::fs::remove_dir_all(&folder);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(files, ["f.txt"]);
    assert_eq!(summary, "c");
    assert_eq!(left, 1);
    assert!(picking.is_ok());
    assert!(!folder_left);
    assert!(branch_left.is_err());
}

/// The last operation: nothing to play again, the new branch is where it was before.
#[test]
fn the_last_operation_leaves_a_branch_where_it_was_before() {
    let dir = repo("remove-last");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let base = git::head(&dir);
    commit(&mut tab, &ctx, "a.txt", "a");
    let last = tab.timeline.current.unwrap_or_default();
    let _ = ask(&mut tab, &ctx, last);
    let outcome = run(&mut tab, &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        matches!(&outcome, Some(RemoveState::Finished { outcome: Outcome::Done { tip, .. }, .. }) if *tip == base),
        "{outcome:?}"
    );
}

/// What can't be removed this way says why: a rewrite (an amend), commits no longer on the
/// branch (reset away since), and later work with a merge in it.
#[test]
fn what_cannot_be_removed_says_why() {
    let dir = repo("remove-refused");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "a.txt", "a");
    tab.amend = true;
    commit(&mut tab, &ctx, "a.txt", "amended");
    let amend = tab.timeline.current.unwrap_or_default();
    let rewrite = ask(&mut tab, &ctx, amend);
    commit(&mut tab, &ctx, "b.txt", "b");
    let made = tab.timeline.current.unwrap_or_default();
    // A merge on main since.
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "side"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    git_ok(
        &dir,
        &["merge", "-q", "--no-ff", "-m", "merge side", "side"],
    );
    tab.reload(&ctx);
    assert!(wait(&mut tab, &ctx, settled));
    let merged = ask(&mut tab, &ctx, made);
    // Reset away.
    git_ok(&dir, &["reset", "-q", "--hard", "HEAD~2"]);
    tab.reload(&ctx);
    assert!(wait(&mut tab, &ctx, settled));
    let gone = ask(&mut tab, &ctx, made);
    let _ = std::fs::remove_dir_all(&dir);
    let error = |s: Option<RemoveState>| match s {
        Some(RemoveState::Failed { error, .. }) => error,
        other => format!("not refused: {other:?}"),
    };
    assert!(error(rewrite).contains("didn't only make commits"));
    assert!(error(merged).contains("merge commit"));
    assert!(error(gone).contains("aren't on main any more"));
}

/// Work done in the worktree after it stopped is not thrown away by Discard; and a branch
/// moved since the plan was read, or a folder in the way, stop it before anything is left
/// behind.
#[test]
fn nothing_is_lost_or_left_behind() {
    let dir = repo("remove-safe");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    commit(&mut tab, &ctx, "f.txt", "a");
    commit(&mut tab, &ctx, "f.txt", "b");
    let middle = tab.timeline.current.unwrap_or_default();
    commit(&mut tab, &ctx, "f.txt", "c");
    let _ = ask(&mut tab, &ctx, middle);
    let Some(RemoveState::Finished {
        outcome: Outcome::Conflict { branch, folder, .. },
        ..
    }) = run(&mut tab, &ctx)
    else {
        unreachable!("no conflict");
    };
    // Resolved there and continued.
    let _ = std::fs::write(folder.join("f.txt"), "resolved\n");
    git_ok(&folder, &["add", "f.txt"]);
    git_ok(
        &folder,
        &["-c", "core.editor=true", "cherry-pick", "--continue"],
    );
    tab.apply(Action::Timeline(TimelineAct::AskDiscard(true)), &ctx);
    act(&mut tab, &ctx, Action::Timeline(TimelineAct::DiscardRemove));
    let refused = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    let kept = git::resolve(&dir, &format!("refs/heads/{branch}")).is_ok() && folder.exists();
    let _ = git::run(
        Some(&dir),
        &["worktree", "remove", "--force", &folder.to_string_lossy()],
    );
    let _ = std::fs::remove_dir_all(&folder);

    // The branch moves after the plan is read: nothing is made.
    let _ = ask(&mut tab, &ctx, middle);
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "since"]);
    let moved = run(&mut tab, &ctx);
    // A folder in the way: no branch is left behind.
    tab.reload(&ctx);
    assert!(wait(&mut tab, &ctx, settled));
    let planned = ask(&mut tab, &ctx, middle);
    let Some(RemoveState::Planned { name, .. }) = planned else {
        unreachable!("{planned:?}");
    };
    let (_, in_the_way) = super::remove::folder(&dir, &name);
    let _ = std::fs::create_dir_all(&in_the_way);
    let _ = std::fs::write(in_the_way.join("x"), "x");
    let blocked = run(&mut tab, &ctx);
    let left = git::resolve(&dir, &format!("refs/heads/{name}"));
    let _ = std::fs::remove_dir_all(&in_the_way);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.contains("Work was done"), "{refused}");
    assert!(kept);
    assert!(
        matches!(&moved, Some(RemoveState::Failed { error, .. }) if error.contains("has moved since")),
        "{moved:?}"
    );
    assert!(
        matches!(blocked, Some(RemoveState::Failed { .. })),
        "{blocked:?}"
    );
    assert!(left.is_err(), "the branch was left behind");
}
