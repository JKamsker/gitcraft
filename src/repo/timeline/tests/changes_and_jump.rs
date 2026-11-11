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
