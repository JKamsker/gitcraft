//! The archaeology on constructed histories: each side's commits read, the first conflicting
//! pair found by test merges (during a merge and a rebase), cancelling, and the tab's flow.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize};

use super::git::timelines;
use super::*;
use crate::git::{Area, StatusFile};
use crate::repo::DiffKey;
use crate::repo::conflict::ConflictEditor;

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    crate::git::run(Some(dir), args)
}

/// The 30 lines of the file, line `n` (1-based) of each changed as `changes` say.
fn lines(changes: &[(usize, &str)]) -> String {
    (1..=30)
        .map(|n| {
            changes
                .iter()
                .rev()
                .find(|(at, _)| *at == n)
                .map_or_else(|| format!("line {n}\n"), |(_, text)| format!("{text}\n"))
        })
        .collect()
}

/// Commits `changes` (so far, on this branch) to `path` as `message`.
fn commit(dir: &Path, path: &str, changes: &[(usize, &str)], message: &str) -> Result<(), String> {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(file, lines(changes)).map_err(|e| e.to_string())?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-qm", message])?;
    Ok(())
}

/// A repository where `main` changed `path` in four commits (and another file in one more) and
/// `side` in three; the third of `main` and the second of `side` change the same line, the rest
/// lines far apart. `main` is checked out.
fn constructed(name: &str, path: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(&dir, args)?;
    }
    commit(&dir, path, &[], "base")?;
    git(&dir, &["checkout", "-qb", "side"])?;
    let mut theirs = vec![(6, "side one")];
    commit(&dir, path, &theirs, "side: one")?;
    theirs.push((12, "side owns line 12"));
    commit(&dir, path, &theirs, "side: change ownership")?;
    theirs.push((18, "side three"));
    commit(&dir, path, &theirs, "side: three")?;
    git(&dir, &["checkout", "-q", "main"])?;
    let mut ours = vec![(2, "main one")];
    commit(&dir, path, &ours, "main: one")?;
    std::fs::write(dir.join("other.txt"), "elsewhere\n").map_err(|e| e.to_string())?;
    git(&dir, &["add", "-A"])?;
    git(&dir, &["commit", "-qm", "main: another file"])?;
    ours.push((25, "main two"));
    commit(&dir, path, &ours, "main: two")?;
    ours.push((12, "main caches line 12"));
    commit(&dir, path, &ours, "main: add caching")?;
    ours.push((28, "main four"));
    commit(&dir, path, &ours, "main: four")?;
    Ok(dir)
}

fn summaries(steps: &[Step]) -> Vec<&str> {
    steps.iter().map(|s| s.summary.as_str()).collect()
}

/// Runs the search on what the timelines list.
fn search(dir: &Path, path: &str, t: &Timelines) -> (Result<Found, Stop>, usize) {
    let (cancel, merges) = (AtomicBool::new(false), AtomicUsize::new(0));
    let found = find(dir, path, t, (&cancel, &merges), &egui::Context::default());
    (found, merges.into_inner())
}

/// A path with unicode and spaces, in a folder.
#[test]
fn a_merge_finds_where_its_conflict_started() -> Result<(), String> {
    let path = "dír ü/the file.txt";
    let dir = constructed("merge", path)?;
    let merged = git(&dir, &["merge", "side"]);
    let timelines = timelines(&dir, path, Some(InProgress::Merge));
    let found = timelines
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(merged.is_err_and(|e| e.contains("CONFLICT")));
    assert_eq!(timelines?.ours.len(), 4);
    assert_eq!(
        found?.0,
        Ok(Found::Pair {
            ours: 2,
            theirs: 1,
            checked: true
        })
    );
    Ok(())
}

#[test]
fn the_first_conflicting_pair_is_found() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("pair", path)?;
    let merged = git(&dir, &["merge", "side"]);
    let timelines = timelines(&dir, path, Some(InProgress::Merge));
    let found = timelines
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let change = timelines
        .as_ref()
        .ok()
        .and_then(|t| t.ours.get(2))
        .map(|s| git::change(&dir, &s.hash, path));
    let _ = std::fs::remove_dir_all(&dir);

    assert!(merged.is_err_and(|e| e.contains("CONFLICT")));
    let t = timelines?;
    // Oldest first; the commit on another file is left out.
    assert_eq!(
        summaries(&t.ours),
        ["main: one", "main: two", "main: add caching", "main: four"]
    );
    assert_eq!(
        summaries(&t.theirs),
        ["side: one", "side: change ownership", "side: three"]
    );
    assert_eq!(t.truncated, (false, false));
    let (found, merges) = found?;
    assert_eq!(
        found,
        Ok(Found::Pair {
            ours: 2,
            theirs: 1,
            checked: true
        })
    );
    assert!(merges <= 6, "{merges} test merges");
    let change = change.ok_or("no change")??;
    assert!(change.contains("+main caches line 12"), "{change}");
    Ok(())
}

/// During a rebase, theirs is the commit being replayed and those before it not yet replayed;
/// what was replayed already (on both sides, as copies) is left out.
#[test]
fn a_rebase_finds_where_its_conflict_started() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("rebase", path)?;
    git(&dir, &["checkout", "-q", "side"])?;
    let rebased = git(&dir, &["rebase", "main"]);
    let timelines = timelines(&dir, path, Some(InProgress::Rebase));
    let found = timelines
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let _ = git(&dir, &["rebase", "--abort"]);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(rebased.is_err());
    let t = timelines?;
    assert_eq!(
        summaries(&t.ours),
        ["main: one", "main: two", "main: add caching", "main: four"]
    );
    assert_eq!(summaries(&t.theirs), ["side: change ownership"]);
    let (found, _) = found?;
    assert_eq!(
        found,
        Ok(Found::Pair {
            ours: 2,
            theirs: 0,
            checked: true
        })
    );
    Ok(())
}

#[test]
fn a_cancelled_search_runs_no_merge() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("cancel", path)?;
    let _ = git(&dir, &["merge", "side"]);
    let t = timelines(&dir, path, Some(InProgress::Merge));
    let (cancel, merges) = (AtomicBool::new(true), AtomicUsize::new(0));
    let found = t
        .as_ref()
        .map(|t| find(&dir, path, t, (&cancel, &merges), &egui::Context::default()));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found?, Err(Stop::Cancelled));
    assert_eq!(merges.into_inner(), 0);
    Ok(())
}

#[test]
fn without_an_operation_there_is_nothing_to_compare() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("none", path)?;
    let none = timelines(&dir, path, Some(InProgress::Merge));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(none.is_err_and(|e| e.contains("MERGE_HEAD")));
    Ok(())
}

// Polls `tab` until `done` holds (or a few seconds pass).
// Source grouping retains the established qualified test names.
include!("tests/tab.rs");

/// A fresh repository with the 30-line file at its base, `main` checked out.
fn based(name: &str, path: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(&dir, args)?;
    }
    commit(&dir, path, &[], "base")?;
    Ok(dir)
}

/// A cherry-pick conflicts on what the picked commit changes from its parent: an earlier
/// commit of its branch that conflicts elsewhere isn't blamed.
#[test]
fn a_cherry_pick_blames_what_the_picked_commit_changes() -> Result<(), String> {
    let path = "f.txt";
    let dir = based("pick", path)?;
    git(&dir, &["checkout", "-qb", "side"])?;
    commit(&dir, path, &[(1, "t1")], "side: line 1")?;
    commit(&dir, path, &[(1, "t1"), (5, "t5")], "side: line 5")?;
    let picked = git(&dir, &["rev-parse", "HEAD"])?;
    git(&dir, &["checkout", "-q", "main"])?;
    commit(&dir, path, &[(1, "o1")], "main: line 1")?;
    commit(&dir, path, &[(1, "o1"), (5, "o5")], "main: line 5")?;
    let _ = git(&dir, &["cherry-pick", picked.trim()]);
    let t = timelines(&dir, path, Some(InProgress::CherryPick));
    let found = t
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let _ = std::fs::remove_dir_all(&dir);
    let t = t?;
    assert_eq!(summaries(&t.ours), ["main: line 1", "main: line 5"]);
    assert_eq!(summaries(&t.theirs), ["side: line 5"]);
    assert_eq!(
        found?.0,
        Ok(Found::Pair {
            ours: 1,
            theirs: 0,
            checked: true
        })
    );
    Ok(())
}

/// Reverting a commit that a later one changed again: the later one is where it started.
#[test]
fn a_revert_blames_the_later_change() -> Result<(), String> {
    let path = "f.txt";
    let dir = based("revert", path)?;
    commit(&dir, path, &[(3, "c1")], "first change")?;
    let reverted = git(&dir, &["rev-parse", "HEAD"])?;
    commit(&dir, path, &[(3, "c2")], "later change")?;
    let _ = git(&dir, &["revert", "--no-edit", reverted.trim()]);
    let t = timelines(&dir, path, Some(InProgress::Revert));
    let found = t
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let _ = std::fs::remove_dir_all(&dir);
    let t = t?;
    assert_eq!(summaries(&t.ours), ["later change"]);
    assert_eq!(summaries(&t.theirs), ["first change"]);
    assert_eq!(
        found?.0,
        Ok(Found::Pair {
            ours: 0,
            theirs: 0,
            checked: true
        })
    );
    Ok(())
}

/// Ours merged another branch in after the commit that conflicts: only its first-parent line
/// is searched, so the commit that started it is found, not the merge; the merge's change is
/// shown from its first parent.
#[test]
fn merges_on_a_side_do_not_hide_where_it_started() -> Result<(), String> {
    let path = "f.txt";
    let dir = based("merged", path)?;
    git(&dir, &["checkout", "-qb", "theirs"])?;
    commit(&dir, path, &[(1, "T")], "theirs: line 1")?;
    git(&dir, &["checkout", "-q", "main"])?;
    git(&dir, &["checkout", "-qb", "other"])?;
    commit(&dir, path, &[(10, "m10")], "other: line 10")?;
    commit(&dir, path, &[(10, "m10"), (13, "m13")], "other: line 13")?;
    git(&dir, &["checkout", "-q", "main"])?;
    commit(&dir, path, &[(1, "S")], "main: line 1")?;
    git(&dir, &["merge", "-q", "--no-edit", "other"])?;
    let _ = git(&dir, &["merge", "theirs"]);
    let t = timelines(&dir, path, Some(InProgress::Merge));
    let found = t
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let merge_change = t
        .as_ref()
        .ok()
        .and_then(|t| t.ours.last())
        .map(|s| git::change(&dir, &s.hash, path));
    let _ = std::fs::remove_dir_all(&dir);
    let t = t?;
    assert_eq!(summaries(&t.ours), ["main: line 1", "Merge branch 'other'"]);
    assert_eq!(
        found?.0,
        Ok(Found::Pair {
            ours: 0,
            theirs: 0,
            checked: true
        })
    );
    let shown = merge_change.ok_or("no merge")??;
    assert!(shown.contains("+m13"), "{shown}");
    Ok(())
}

/// Both sides made the same change, then theirs changed it again: a merge keeps ours' copy, so
/// the pair is found rather than "only one side".
#[test]
fn the_same_change_on_both_sides_is_kept_for_a_merge() -> Result<(), String> {
    let path = "f.txt";
    let dir = based("same", path)?;
    git(&dir, &["checkout", "-qb", "theirs"])?;
    commit(&dir, path, &[(4, "X")], "theirs: X")?;
    commit(&dir, path, &[(4, "Y")], "theirs: Y")?;
    git(&dir, &["checkout", "-q", "main"])?;
    commit(&dir, path, &[(4, "X")], "main: X")?;
    let _ = git(&dir, &["merge", "theirs"]);
    let t = timelines(&dir, path, Some(InProgress::Merge));
    let found = t
        .as_ref()
        .map(|t| search(&dir, path, t))
        .map_err(String::clone);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(summaries(&t?.ours), ["main: X"]);
    assert_eq!(
        found?.0,
        Ok(Found::Pair {
            ours: 0,
            theirs: 1,
            checked: true
        })
    );
    Ok(())
}

/// A first commit picked has no parent to merge from: said, not a raw git error.
#[test]
fn a_picked_first_commit_says_there_is_no_history() -> Result<(), String> {
    let path = "f.txt";
    let dir = based("root", path)?;
    git(&dir, &["checkout", "-q", "--orphan", "other"])?;
    git(&dir, &["rm", "-q", "-rf", "."])?;
    commit(&dir, path, &[(1, "other")], "other root")?;
    let root = git(&dir, &["rev-parse", "HEAD"])?;
    git(&dir, &["checkout", "-q", "main"])?;
    let _ = git(&dir, &["cherry-pick", root.trim()]);
    let t = timelines(&dir, path, Some(InProgress::CherryPick));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(t.is_err_and(|e| e.contains("no parent")));
    Ok(())
}

/// Pair A, then B, then A again while A still runs: A is merged once, and its answer still
/// lands when it comes.
#[test]
fn a_pair_in_flight_is_not_merged_twice() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("inflight", path)?;
    let _ = git(&dir, &["merge", "side"]);
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.work.in_progress = Some(InProgress::Merge);
    let file = StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area: Area::Conflicted,
    };
    tab.open_diff(DiffKey::Work(file), &ctx);
    tab.archaeology_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.conflict
            .as_ref()
            .is_some_and(|ed| matches!(ed.history.timelines, Read::Done(_)))
    });
    // Answers caught, to count the merges started.
    let (tx, rx) = std::sync::mpsc::channel();
    tab.tx.sender = tx;
    tab.archaeology_act(Act::TestPair(2, 1), &ctx);
    tab.archaeology_act(Act::TestPair(0, 0), &ctx);
    tab.archaeology_act(Act::TestPair(2, 1), &ctx);
    let mut answers = Vec::new();
    while let Ok(msg) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
        answers.push(msg);
    }
    for msg in std::mem::take(&mut answers).into_iter().rev() {
        if let crate::repo::messages::Msg::Then(then) = msg {
            then(&mut tab, &ctx);
            answers.push(crate::repo::messages::Msg::JobPanicked(String::new()));
        }
    }
    let shown = tab
        .conflict
        .as_ref()
        .and_then(|ed| ed.history.pair_test.clone());
    let left = tab.conflict.as_ref().map(|ed| ed.history.testing.len());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(answers.len(), 2, "two merges, not three");
    assert_eq!(shown, Some(((2, 1), Read::Done(true))));
    assert_eq!(left, Some(0));
    Ok(())
}
