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
