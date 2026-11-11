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
