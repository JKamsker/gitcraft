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

/// Polls `tab` until `done` holds (or a few seconds pass).
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Through the tab, as the conflict editor asks: the timelines are read, the pair found and a
/// commit's change shown; an editor opened again meanwhile takes none of it.
#[test]
fn the_tab_reads_and_searches_for_the_open_editor() -> Result<(), String> {
    let path = "f.txt";
    let dir = constructed("tab", path)?;
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
    fn history(tab: &RepoTab) -> Option<&History> {
        tab.conflict.as_ref().map(|ed| &ed.history)
    }
    tab.archaeology_act(Act::Load, &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab).is_some_and(|h| matches!(h.timelines, Read::Done(_)))
    });
    tab.archaeology_act(Act::Find, &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab)
            .and_then(|h| h.search.as_ref())
            .is_some_and(|s| s.result != Read::Reading)
    });
    let found = history(&tab)
        .and_then(|h| h.search.as_ref())
        .map(|s| s.result.clone());
    let hash = match history(&tab).map(|h| &h.timelines) {
        Some(Read::Done(t)) => t.ours.first().map(|s| s.hash.clone()),
        _ => None,
    }
    .ok_or("no commits")?;
    tab.archaeology_act(Act::ShowChange(hash.clone(), "short".to_owned()), &ctx);
    wait(&mut tab, &ctx, |tab| {
        history(tab)
            .and_then(|h| h.change.as_ref())
            .is_some_and(|(_, _, r)| *r != Read::Reading)
    });
    let shown = history(&tab).and_then(|h| h.change.clone());
    // Asked again for the same commit: hidden.
    tab.archaeology_act(Act::ShowChange(hash.clone(), "short".to_owned()), &ctx);
    let hidden = history(&tab).is_some_and(|h| h.change.is_none());

    // A read for an earlier opening isn't taken by the one open now.
    tab.archaeology_act(Act::Load, &ctx);
    if let Some(ed) = tab.conflict.as_mut() {
        ed.generation += 1;
        ed.history = History::default();
    }
    for _ in 0..40 {
        tab.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let untouched = history(&tab).is_some_and(|h| h.timelines == Read::NotAsked);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        found,
        Some(Read::Done(Found::Pair {
            ours: 2,
            theirs: 1,
            checked: true
        }))
    );
    match shown {
        Some((h, _, Read::Done(text))) => {
            assert_eq!(h, hash);
            assert!(text.contains("+main one"), "{text}");
        }
        other => return Err(format!("{other:?}")),
    }
    assert!(hidden);
    assert!(untouched);
    Ok(())
}

/// Drawn: the editor asks for the timelines as it shows, lists each side's commits with the
/// pair found lit and named, and a click on a commit asks for its change.
#[test]
fn the_strip_shows_both_sides_and_the_answer() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-drawn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("f"), "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n")
        .map_err(|e| e.to_string())?;
    let ed = ConflictEditor::open(&dir, "f", Some(InProgress::Merge)).ok_or("not opened")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    tab.conflict = Some(ed);
    tab.work.in_progress = Some(InProgress::Merge);
    let draw = |ui: &mut egui::Ui, (tab, acts, fonts): &mut State| {
        // gitcraft's fonts first: they apply from the next frame.
        if !*fonts {
            crate::theme::install_fonts(ui.ctx());
            *fonts = true;
            return;
        }
        tab.conflict_view(ui, acts);
    };
    let mut harness = egui_kittest::Harness::new_ui_state(draw, (tab, Vec::new(), false));
    harness.run_steps(2);
    let asked = harness
        .state()
        .1
        .iter()
        .any(|a| matches!(a, crate::repo::Action::Archaeology(Act::Load)));
    let step = |short: &str, summary: &str| Step {
        hash: format!("{short}ffff"),
        short: short.to_owned(),
        author: "Zoë".to_owned(),
        time: 0,
        summary: summary.to_owned(),
    };
    if let Some(ed) = harness.state_mut().0.conflict.as_mut() {
        ed.history.timelines = Read::Done(Timelines {
            ours: vec![step("a1", "add caching"), step("a2", "tune")],
            theirs: vec![step("b1", "change ownership")],
            truncated: (false, false),
            base: None,
            theirs_rev: None,
        });
        ed.history.search = Some(Search {
            cancel: Default::default(),
            merges: Default::default(),
            result: Read::Done(Found::Pair {
                ours: 0,
                theirs: 0,
                checked: true,
            }),
        });
    }
    harness.run();
    harness.get_by_label("Find where it started");
    harness.get_by_label_contains("First conflicting pair: a1 “add caching”");
    // Picking a commit on each side offers its change and a test merge of the two.
    harness.get_by_label_contains("a2  tune").click();
    harness.run();
    harness
        .get_by_label_contains("b1  change ownership")
        .click();
    harness.run();
    harness.get_by_label("What b1 changed").click();
    harness.get_by_label("Test these two").click();
    harness.run();
    let state = harness.state();
    let acts = &state.1;
    assert!(asked);
    assert!(acts.iter().any(|a| matches!(a,
        crate::repo::Action::Archaeology(Act::ShowChange(h, s)) if h == "b1ffff" && s == "b1")));
    assert!(
        acts.iter()
            .any(|a| matches!(a, crate::repo::Action::Archaeology(Act::TestPair(1, 0))))
    );
    Ok(())
}

/// A stash popped with conflicts (no operation) has no other branch: no strip, nothing read.
#[test]
fn a_conflict_from_no_branch_shows_no_strip() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    type State = (RepoTab, Vec<crate::repo::Action>, bool);
    let dir = std::env::temp_dir().join(format!("gitcraft-arch-stash-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("f"), "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n")
        .map_err(|e| e.to_string())?;
    let ed = ConflictEditor::open(&dir, "f", None).ok_or("not opened")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    tab.conflict = Some(ed);
    let draw = |ui: &mut egui::Ui, (tab, acts, fonts): &mut State| {
        if !*fonts {
            crate::theme::install_fonts(ui.ctx());
            *fonts = true;
            return;
        }
        tab.conflict_view(ui, acts);
    };
    let mut harness = egui_kittest::Harness::new_ui_state(draw, (tab, Vec::new(), false));
    harness.run_steps(2);
    assert!(
        harness
            .query_by_label("HOW THIS CONFLICT CAME ABOUT")
            .is_none()
    );
    assert!(
        !harness
            .state()
            .1
            .iter()
            .any(|a| matches!(a, crate::repo::Action::Archaeology(_)))
    );
    Ok(())
}

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
