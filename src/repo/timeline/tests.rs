//! Tests of the timeline's model: the tree undo and redo walk, its cap, keeping it on disk,
//! taking in what was done before it was read, the steps of a jump, and reading the reflog.

use std::collections::BTreeMap;

use super::observe::{describe_reflog, fallback_label, parse_reflog};
use super::snapshot::{commits_of, describe, restore_steps};
use super::store::{self, Saved};
use super::{Entry, MAX_ENTRIES, Origin, RefState, Timeline, changes, tree};
use crate::repo::UndoEntry;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn state(branch: &str, branches: &[(&str, &str)]) -> RefState {
    let branches: BTreeMap<String, String> = branches
        .iter()
        .map(|(n, c)| ((*n).to_owned(), (*c).to_owned()))
        .collect();
    RefState {
        branch: branch.to_owned(),
        head: branches.get(branch).cloned().unwrap_or_default(),
        branches,
    }
}

fn entry(label: &str) -> Entry {
    Entry {
        id: 0,
        parent: None,
        time: 0,
        label: label.to_owned(),
        origin: Origin::Gitcraft,
        before: RefState::default(),
        after: RefState::default(),
        files: Vec::new(),
        files_total: 0,
        undo: None,
        redo_child: None,
        receipt: None,
    }
}

fn labels(t: &Timeline, ids: &[u64]) -> Vec<String> {
    ids.iter()
        .filter_map(|id| t.get(*id).map(|e| e.label.clone()))
        .collect()
}

/// Undo three steps and do something new: the three undone stay, as a branch of the tree, and
/// redo from where they hang off goes down the newest one.
#[test]
fn something_new_after_undoing_starts_a_branch_and_keeps_the_old_future() {
    let mut t = Timeline::default();
    let one = t.push(entry("one"));
    let two = t.push(entry("two"));
    let three = t.push(entry("three"));
    let four = t.push(entry("four"));
    // Undo three, four… down to one.
    for _ in 0..3 {
        let parent = t.current_entry().and_then(|e| e.parent);
        t.move_to(parent, None);
    }
    assert_eq!(t.current, Some(one));
    assert_eq!(t.redo_target().map(|e| e.id), Some(two));
    let five = t.push(entry("five"));
    assert_eq!(t.get(five).and_then(|e| e.parent), Some(one));
    assert_eq!(t.entries.len(), 5);
    assert_eq!(
        labels(&t, &t.path_to(Some(four))),
        ["one", "two", "three", "four"]
    );
    assert!(!t.is_done(two));
    assert!(t.is_done(five));
    // Back to one, redo goes to the newest child, five.
    t.move_to(Some(one), None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(five));
    // A jump into the old future: redo from there goes on down it.
    t.move_to(Some(two), None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(three));
    // The tree: five on a lane of its own, the old future in the first.
    let (rows, lanes) = tree::layout(&t);
    assert_eq!(lanes, 2);
    let lane = |id: u64| rows.iter().find(|r| r.id == Some(id)).map(|r| r.lane);
    assert_eq!(lane(two), Some(0));
    assert_eq!(lane(four), Some(0));
    assert_eq!(lane(five), Some(1));
    // Rows newest first, then the start.
    assert_eq!(rows.first().and_then(|r| r.id), Some(five));
    assert_eq!(rows.last().map(|r| r.id), Some(None));
}

/// Redo comes back to the child last undone from, not the newest one.
#[test]
fn redo_comes_back_down_the_branch_last_undone() {
    let mut t = Timeline::default();
    let base = t.push(entry("base"));
    let old = t.push(entry("old"));
    t.move_to(Some(base), None);
    let new = t.push(entry("new"));
    t.move_to(Some(base), None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(new));
    // Over to the old branch and back up from it.
    t.move_to(Some(old), None);
    t.move_to(Some(base), None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(old));
    // Undone all the way to before the first entry, redo goes to it.
    t.move_to(None, None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(base));
}

/// Moving with an updated way to undo (its checks filled by the run) keeps it on the entry.
#[test]
fn an_undo_that_ran_keeps_its_updated_commands() {
    let mut t = Timeline::default();
    let mut e = entry("stash");
    e.undo = Some(UndoEntry {
        agent: None,
        id: 0,
        label: "stash".to_owned(),
        undo: vec![vec!["stash".to_owned(), "pop".to_owned()]],
        redo: Vec::new(),
        heads: None,
    });
    let id = t.push(e);
    assert_eq!(
        t.get(id).and_then(|e| e.undo.as_ref()).map(|u| u.id),
        Some(id)
    );
    let mut updated = t.get(id).and_then(|e| e.undo.clone());
    if let Some(u) = &mut updated {
        u.redo = vec![vec!["stash".to_owned(), "push".to_owned()]];
    }
    t.move_to(None, updated.clone());
    assert_eq!(t.get(id).and_then(|e| e.undo.clone()), updated);
    assert_eq!(t.current, None);
}

/// The timeline is capped: the oldest go, never the current one, and what hung from them hangs
/// from what they hung from; their kept commits are let go of.
#[test]
fn the_oldest_entries_go_past_the_cap() {
    let mut t = Timeline::default();
    let first = t.push(entry("first"));
    for n in 0..MAX_ENTRIES + 4 {
        t.push(entry(&format!("e{n}")));
    }
    assert_eq!(t.entries.len(), MAX_ENTRIES);
    assert!(t.get(first).is_none());
    assert_eq!(t.dropped.len(), 5);
    assert!(t.dropped.contains(&first));
    let oldest = t.entries.first();
    assert_eq!(oldest.and_then(|e| e.parent), None);
    assert_eq!(t.current_entry().map(|e| e.label.as_str()), Some("e303"));
}

/// Forgetting older entries keeps the newest and the path to the current one.
#[test]
fn forgetting_keeps_the_newest_and_the_way_to_the_current_one() {
    let mut t = Timeline::default();
    let root = t.push(entry("root"));
    let kept = t.push(entry("kept"));
    for n in 0..10 {
        t.move_to(Some(root), None);
        t.push(entry(&format!("side {n}")));
    }
    t.move_to(Some(kept), None);
    t.forget_older(3);
    let left: Vec<&str> = t.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(left, ["root", "kept", "side 7", "side 8", "side 9"]);
    assert_eq!(t.current, Some(kept));
    t.forget(&t.entries.iter().map(|e| e.id).collect::<Vec<_>>());
    assert!(t.entries.is_empty());
    assert_eq!(t.current, None);
}

/// Kept on disk and read back, the timeline is the same, unicode and all.
#[test]
fn the_timeline_reads_back_as_it_was_written() {
    let dir = std::env::temp_dir().join(format!("gitcraft-tl-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("ü dir").join("t.json");
    let mut t = Timeline::default();
    let mut e = entry("Commit “Füße and spaces”");
    e.before = state("main", &[("main", A)]);
    e.after = state("main", &[("main", B), ("fëature/ünï", C)]);
    e.files = vec!["dir with space/ü.txt".to_owned()];
    e.files_total = 1;
    t.push(e);
    t.push(entry(&"x".repeat(10_000)));
    t.seen = Some(state("main", &[("main", B)]));
    let saved = Saved::of(&t, std::path::Path::new("/repo"));
    let written = store::save_to(&path, &saved, store::next_generation());
    let read = store::load_from(&path);
    // An older write that comes late is not written over the newer one.
    let mut stale = saved.clone();
    stale.entries.clear();
    let skipped = store::save_to(&path, &stale, 0);
    let after_stale = store::load_from(&path);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(written, Ok(()));
    assert_eq!(read, Ok(saved.clone()));
    assert_eq!(skipped, Ok(()));
    assert_eq!(after_stale, Ok(saved));
}

/// No file yet is an empty timeline; one damaged or from a newer gitcraft is an error, said so.
#[test]
fn a_file_that_cannot_be_read_is_said_so() {
    let dir = std::env::temp_dir().join(format!("gitcraft-tl-bad-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let missing = store::load_from(&dir.join("none.json"));
    let damaged = dir.join("damaged.json");
    let _ = std::fs::write(&damaged, "{not json");
    let newer = dir.join("newer.json");
    let _ = std::fs::write(
        &newer,
        r#"{"version":99,"entries":[],"current":null,"seen":null,"next_id":0}"#,
    );
    let (damaged, newer) = (store::load_from(&damaged), store::load_from(&newer));
    let empty = dir.join("empty.json");
    let _ = std::fs::write(&empty, "");
    let empty = store::load_from(&empty);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(missing, Ok(Saved::default()));
    assert!(damaged.is_err_and(|e| e.contains("damaged")));
    assert!(newer.is_err_and(|e| e.contains("newer gitcraft")));
    assert!(empty.is_err());
}

/// What the tab did before what was on disk came in (the in-memory journal) goes after the saved
/// entries, under the saved current one, with new ids and its own links kept.
#[test]
fn what_was_done_before_the_file_came_in_goes_after_it() {
    let mut saved_t = Timeline::default();
    let s1 = saved_t.push(entry("saved one"));
    let s2 = saved_t.push(entry("saved two"));
    saved_t.seen = Some(state("main", &[("main", A)]));
    let saved = Saved::of(&saved_t, std::path::Path::new("/repo"));

    let mut t = Timeline::default();
    let mut undoable = entry("mine one");
    undoable.undo = Some(UndoEntry {
        agent: None,
        id: 0,
        label: "mine one".to_owned(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    });
    let m1 = t.push(undoable);
    let m2 = t.push(entry("mine two"));
    t.move_to(Some(m1), None);
    t.seen = Some(state("main", &[("main", B)]));
    t.adopt(saved);

    assert!(t.loaded);
    assert_eq!(t.entries.len(), 4);
    let id_of = |label: &str| t.entries.iter().find(|e| e.label == label).map(|e| e.id);
    let (n1, n2) = (id_of("mine one"), id_of("mine two"));
    assert!(n1.is_some_and(|id| id > s2));
    assert_ne!((n1, n2), (Some(m1), Some(m2)));
    assert_eq!(n1.and_then(|id| t.get(id)).and_then(|e| e.parent), Some(s2));
    assert_eq!(n2.and_then(|id| t.get(id)).and_then(|e| e.parent), n1);
    assert_eq!(t.current, n1);
    // Its undo knows its new id; redo from it goes down to "mine two".
    assert_eq!(
        n1.and_then(|id| t.get(id))
            .and_then(|e| e.undo.as_ref())
            .map(|u| u.id),
        n1
    );
    assert_eq!(t.redo_target().map(|e| e.id), n2);
    // What this tab read last is newer than what was kept.
    assert_eq!(t.seen, Some(state("main", &[("main", B)])));
    assert_eq!(t.get(s1).map(|e| e.label.as_str()), Some("saved one"));
    // A new entry gets an id past them all.
    let next = t.push(entry("next"));
    assert!(next > n2.unwrap_or(u64::MAX));
}

/// With nothing done before it, the file is taken as it is.
#[test]
fn adopting_with_nothing_of_its_own_takes_the_file_as_it_is() {
    let mut saved_t = Timeline::default();
    saved_t.push(entry("one"));
    let two = saved_t.push(entry("two"));
    saved_t.move_to(None, None);
    saved_t.seen = Some(state("main", &[("main", A)]));
    let saved = Saved::of(&saved_t, std::path::Path::new("/r"));
    let mut t = Timeline::default();
    t.adopt(saved);
    assert_eq!(t.entries.len(), 2);
    assert_eq!(t.current, None);
    assert_eq!(t.seen, Some(state("main", &[("main", A)])));
    assert!(t.get(two).is_some());
}

#[test]
fn changes_list_branches_made_moved_and_deleted_then_head() {
    let before = state("main", &[("main", A), ("old", B)]);
    let after = state("new", &[("main", B), ("new", C)]);
    let list = changes(&before, &after);
    let names: Vec<(&str, &str, &str)> = list
        .iter()
        .map(|c| (c.name.as_str(), c.old.as_str(), c.new.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("main", A, B),
            ("new", "", C),
            ("old", B, ""),
            ("HEAD", "main", "new")
        ]
    );
    // A detached HEAD that moved.
    let mut d1 = state("", &[("main", A)]);
    d1.head = A.to_owned();
    let mut d2 = d1.clone();
    d2.head = B.to_owned();
    assert_eq!(changes(&d1, &d2).len(), 1);
    assert!(changes(&d1, &d1).is_empty());
}

/// The kept commits of an entry are those it moved branches from and to, once each.
#[test]
fn an_entry_keeps_the_commits_it_moved_from_and_to() {
    let mut e = entry("rebase");
    e.before = state("main", &[("main", A), ("x", C)]);
    e.after = state("main", &[("main", B), ("x", C)]);
    assert_eq!(commits_of(&e), [A, B]);
    e.after = state("main", &[("main", A), ("x", C)]);
    assert!(commits_of(&e).is_empty());
}

/// A jump moves the other branches first, then HEAD; the branch checked out is reset with
/// `--keep`; everything it touches is checked before anything runs.
#[test]
fn a_jump_checks_first_then_moves_branches_then_head() {
    let s = |args: &[&str]| {
        args.iter()
            .map(|a| (*a).to_owned())
            .collect::<Vec<String>>()
    };
    let now = state("main", &[("main", B), ("feat", C)]);
    let target = state("main", &[("main", A), ("gone", C)]);
    let steps = restore_steps(&now, &target, &[]);
    let Ok(steps) = steps else {
        unreachable!("{steps:?}");
    };
    let runs: Vec<&Vec<String>> = steps.iter().filter(|s| !s[0].starts_with('\0')).collect();
    assert_eq!(
        runs,
        [
            &s(&[
                "update-ref",
                "-m",
                "gitcraft: undo tree",
                "-d",
                "refs/heads/feat",
                C
            ]),
            &s(&[
                "update-ref",
                "-m",
                "gitcraft: undo tree",
                "refs/heads/gone",
                C,
                ""
            ]),
            &s(&["reset", "--keep", A]),
        ]
    );
    // The checks: HEAD on main, and each branch where it was read.
    assert_eq!(steps.len() - runs.len(), 4);
    // To another branch: that one is made first, HEAD goes there, then the old one moves.
    let target = state("feat", &[("main", A), ("feat", C)]);
    let Ok(steps) = restore_steps(&now, &target, &[]) else {
        unreachable!();
    };
    let runs: Vec<&Vec<String>> = steps.iter().filter(|s| !s[0].starts_with('\0')).collect();
    assert_eq!(
        runs,
        [
            &s(&["checkout", "feat", "--"]),
            &s(&[
                "update-ref",
                "-m",
                "gitcraft: undo tree",
                "refs/heads/main",
                A,
                B
            ]),
        ]
    );
    // Detached.
    let mut detached = target.clone();
    detached.branch.clear();
    detached.head = A.to_owned();
    let Ok(steps) = restore_steps(&now, &detached, &[]) else {
        unreachable!();
    };
    assert!(steps.contains(&s(&["checkout", "--detach", A, "--"])));
    // What it says it changes.
    assert_eq!(
        describe(&now, &target),
        ["main: bbbbbbbb → aaaaaaaa", "Checked out: main → feat"]
    );
}

#[test]
fn a_jump_refuses_what_it_cannot_do() {
    let now = state("main", &[("main", B), ("wt", C)]);
    // A branch checked out in another worktree.
    let target = state("main", &[("main", B), ("wt", A)]);
    assert!(
        restore_steps(&now, &target, &["wt".to_owned()]).is_err_and(|e| e.contains("worktree"))
    );
    // The branch checked out, gone then.
    let target = state("main", &[("wt", C)]);
    assert!(restore_steps(&now, &target, &[]).is_err_and(|e| e.contains("no commit yet")));
    // Before the first commit.
    assert!(restore_steps(&now, &RefState::default(), &[]).is_err());
    // Nothing to change: only checks.
    let Ok(same) = restore_steps(&now, &now, &[]) else {
        unreachable!();
    };
    assert!(same.iter().all(|s| s[0].starts_with('\0')));
}

#[test]
fn the_reflog_is_read_back_to_where_it_was_last_seen() {
    let out = format!("{B}\x1fcommit: two\n{A}\x1fcommit: one\n{C}\x1fcommit (initial): base\n");
    assert_eq!(parse_reflog(&out, A, false), ["commit: two"]);
    assert_eq!(parse_reflog(&out, B, false), Vec::<String>::new());
    // A checkout to a branch at the same commit: the newest entry.
    assert_eq!(parse_reflog(&out, B, true), ["commit: two"]);
    // Not found within what was read: nothing is guessed.
    assert!(parse_reflog(&out, "dddd", false).is_empty());
    assert_eq!(parse_reflog("", A, false), Vec::<String>::new());
    assert_eq!(
        describe_reflog("commit: Fix the “parser”"),
        "Commit “Fix the “parser””"
    );
    assert_eq!(
        describe_reflog("rebase (finish): returning to refs/heads/main"),
        "Rebase"
    );
    assert_eq!(
        describe_reflog("reset: moving to HEAD~1"),
        "Reset to HEAD~1"
    );
    assert_eq!(
        describe_reflog("checkout: moving from main to fëature"),
        "Checkout fëature"
    );
    assert_eq!(describe_reflog("merge feat: Fast-forward"), "Merge feat");
    assert_eq!(describe_reflog("pull --rebase: x"), "Pull");
    assert_eq!(describe_reflog("odd"), "odd");
    assert_eq!(describe_reflog(""), "Moved");
}

#[test]
fn without_a_reflog_the_label_says_which_branches_moved() {
    let label = |before: &RefState, after: &RefState| fallback_label(&changes(before, after));
    let a = state("main", &[("main", A)]);
    assert_eq!(label(&a, &state("main", &[("main", B)])), "main moved");
    assert_eq!(
        label(&a, &state("main", &[("main", A), ("n", B)])),
        "Branch n made"
    );
    assert_eq!(
        label(&state("main", &[("main", A), ("n", B)]), &a),
        "Branch n deleted"
    );
    assert_eq!(
        label(&a, &state("main", &[("main", B), ("n", B)])),
        "main moved, and 1 more"
    );
    let mut detached = a.clone();
    detached.branch.clear();
    assert_eq!(label(&a, &detached), "Checkout aaaaaaaa");
}

/// The state at an entry is worked out from the one now, up the tree and down to it: branches
/// no entry on the way touched stay as they are now.
#[test]
fn the_state_at_an_entry_is_walked_from_the_current_one() {
    let mut t = Timeline::default();
    let push = |t: &mut Timeline, label: &str, before: RefState, after: RefState| {
        let (before, after) = super::sparse(&before, &after);
        let mut e = entry(label);
        e.before = before;
        e.after = after;
        t.push(e)
    };
    // main A → B, then feat made at C.
    let s0 = state("main", &[("main", A), ("other", A)]);
    let s1 = state("main", &[("main", B), ("other", A)]);
    let s2 = state("feat", &[("main", B), ("feat", C), ("other", A)]);
    let one = push(&mut t, "one", s0.clone(), s1.clone());
    let two = push(&mut t, "two", s1.clone(), s2.clone());
    // Kept sparse: only what changed.
    assert_eq!(t.get(one).map(|e| e.after.branches.len()), Some(1));
    // Undone to one, then a side branch: main A… back to B → C.
    t.move_to(Some(one), None);
    let s3 = state("main", &[("main", C), ("other", A)]);
    let side = push(&mut t, "side", s1.clone(), s3.clone());
    // `other` moved since, outside any entry: it stays as it is now.
    let mut now = s3.clone();
    now.branches.insert("other".to_owned(), B.to_owned());
    let at_two = t.state_at(&now, Some(two));
    assert_eq!(at_two.branch, "feat");
    assert_eq!(at_two.branches.get("main").map(String::as_str), Some(B));
    assert_eq!(at_two.branches.get("feat").map(String::as_str), Some(C));
    assert_eq!(at_two.branches.get("other").map(String::as_str), Some(B));
    let start = t.state_at(&now, None);
    assert_eq!(start.branches.get("main").map(String::as_str), Some(A));
    assert!(!start.branches.contains_key("feat"));
    assert_eq!(t.state_at(&now, Some(side)), now);
    // Forgetting the first entry: the start is where it left the branches.
    t.forget(&[one]);
    let start = t.state_at(&now, None);
    assert_eq!(start.branches.get("main").map(String::as_str), Some(B));
}

/// A jump refuses branch names git would read as options, and a branch to check out that is
/// checked out in another worktree; a branch to check out that isn't there now is made first.
#[test]
fn a_jump_checks_the_branch_it_checks_out() {
    let now = state("main", &[("main", B)]);
    let mut target = state("-f", &[("main", B), ("-f", A)]);
    target.head = A.to_owned();
    assert!(restore_steps(&now, &target, &[]).is_err_and(|e| e.contains("option")));
    let now = state("main", &[("main", B), ("feat", C)]);
    let target = state("feat", &[("main", B), ("feat", C)]);
    assert!(
        restore_steps(&now, &target, &["feat".to_owned()]).is_err_and(|e| e.contains("worktree"))
    );
    let target = state("new", &[("main", B), ("feat", C), ("new", A)]);
    let Ok(steps) = restore_steps(&now, &target, &[]) else {
        unreachable!();
    };
    let runs: Vec<&Vec<String>> = steps.iter().filter(|s| !s[0].starts_with('\0')).collect();
    let made = vec![
        "update-ref".to_owned(),
        "-m".to_owned(),
        "gitcraft: undo tree".to_owned(),
        "refs/heads/new".to_owned(),
        A.to_owned(),
        String::new(),
    ];
    let checkout = vec!["checkout".to_owned(), "new".to_owned(), "--".to_owned()];
    assert_eq!(runs, [&made, &checkout]);
}

/// A jump past an entry only commands undo (a stash) would leave it as it is while the tree
/// says otherwise: it is named, to step through instead.
#[test]
fn entries_that_moved_no_branch_are_found_on_a_jumps_way() {
    let mut t = Timeline::default();
    let mut stash = entry("Stash");
    stash.undo = Some(UndoEntry {
        agent: None,
        id: 0,
        label: "Stash".to_owned(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    });
    let s = t.push(stash);
    let mut c = entry("Commit");
    c.before = state("main", &[("main", A)]);
    c.after = state("main", &[("main", B)]);
    let commit = t.push(c);
    assert_eq!(
        t.command_only_between(Some(commit), None),
        Some("Stash".to_owned())
    );
    assert_eq!(t.command_only_between(Some(commit), Some(s)), None);
    assert_eq!(t.command_only_between(Some(s), Some(commit)), None);
}

/// Redo from before every entry goes back to the first-level entry last undone from.
#[test]
fn redo_from_the_start_goes_back_to_the_entry_undone_from() {
    let mut t = Timeline::default();
    let r1 = t.push(entry("r1"));
    t.move_to(None, None);
    let r2 = t.push(entry("r2"));
    assert_eq!(t.get(r2).and_then(|e| e.parent), None);
    t.move_to(Some(r1), None);
    t.move_to(None, None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(r1));
    t.move_to(Some(r2), None);
    t.move_to(None, None);
    assert_eq!(t.redo_target().map(|e| e.id), Some(r2));
}

/// Undoing and doing something else again and again doesn't pile up lanes.
#[test]
fn lanes_are_used_again() {
    let mut t = Timeline::default();
    let base = t.push(entry("base"));
    let mut at = base;
    for n in 0..30 {
        let done = t.push(entry(&format!("a{n}")));
        t.move_to(Some(at), None);
        at = t.push(entry(&format!("b{n}")));
        let _ = done;
    }
    let (_, lanes) = tree::layout(&t);
    assert!(lanes <= 3, "{lanes} lanes");
}
