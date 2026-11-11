#[test]
fn undoing_a_stage_puts_back_what_was_staged_before() {
    let dir = repo("stage");
    // Partly staged already: the first change, then a second one not staged.
    write(&dir, "a.txt", "one\ntwo\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    write(&dir, "a.txt", "one\ntwo\nthree\n");
    write(&dir, "new.txt", "new\n");
    let index_before = git_out(&dir, &["ls-files", "-s"]);
    let p = planned(
        &dir,
        WriteOp::Stage {
            paths: vec!["a.txt".into(), "new.txt".into()],
        },
    );
    assert!(p.ask.is_none());
    let entry = run(&dir, &p);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let index_after = git_out(&dir, &["ls-files", "-s"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(staged, "a.txt\nnew.txt\n");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(index_after, index_before, "the index is exactly as before");
}

#[test]
fn undoing_an_unstage_stages_the_same_again() {
    let dir = repo("unstage");
    write(&dir, "a.txt", "changed\n");
    write(&dir, "b.txt", "added\n");
    assert!(git::run(Some(&dir), &["add", "-A"]).is_ok());
    let index_before = git_out(&dir, &["ls-files", "-s"]);
    let p = planned(
        &dir,
        WriteOp::Unstage {
            paths: vec!["a.txt".into(), "b.txt".into()],
        },
    );
    let entry = run(&dir, &p);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let index_after = git_out(&dir, &["ls-files", "-s"]);
    let nothing = plan(
        &dir,
        &WriteOp::Unstage {
            paths: vec!["a.txt".into()],
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(staged, "");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(index_after, index_before);
    // Staged again by the undo: there is something to unstage once more.
    assert!(nothing.is_ok());
}

#[test]
fn a_commit_is_undone_with_its_changes_kept_staged() {
    let dir = repo("commit");
    let empty = plan(
        &dir,
        &WriteOp::Commit {
            message: "x".into(),
            amend: false,
        },
    );
    write(&dir, "a.txt", "two\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    let base = git::head(&dir);
    let p = planned(
        &dir,
        WriteOp::Commit {
            message: "Fix the login\n\nBecause.".into(),
            amend: false,
        },
    );
    let entry = run(&dir, &p);
    let message = git_out(&dir, &["log", "-1", "--format=%B"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = git::head(&dir);
    let staged = git_out(&dir, &["diff", "--cached", "--name-only"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(empty.is_err_and(|e| e.contains("nothing is staged")));
    // Not "Commit": that label clears the commit box the user may be writing in.
    assert_eq!(p.label, "Commit “Fix the login”");
    assert_eq!(message.trim(), "Fix the login\n\nBecause.");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(back, base);
    assert_eq!(staged, "a.txt\n");
}

#[test]
fn branches_are_made_renamed_and_checked_out_undoably() {
    let dir = repo("branches");
    let p = planned(
        &dir,
        WriteOp::CreateBranch {
            name: "feature".into(),
            start: None,
            checkout: true,
        },
    );
    let made = run(&dir, &p);
    let on = git_out(&dir, &["branch", "--show-current"]);
    let undone = made
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let gone = git::resolve(&dir, "refs/heads/feature").is_err();
    let back_on = git_out(&dir, &["branch", "--show-current"]);
    let refused = [
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "main".into(),
                start: None,
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "-f".into(),
                start: None,
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::CreateBranch {
                name: "x".into(),
                start: Some("--orphan".into()),
                checkout: false,
            },
        ),
        plan(
            &dir,
            &WriteOp::Checkout {
                branch: "main".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::Checkout {
                branch: "nope".into(),
            },
        ),
    ];
    assert!(git::run(Some(&dir), &["branch", "side"]).is_ok());
    let renamed = plan(
        &dir,
        &WriteOp::RenameBranch {
            from: "side".into(),
            to: "other".into(),
        },
    )
    .map(|p| run(&dir, &p));
    let other = git::resolve(&dir, "refs/heads/other").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(p.ask.is_none());
    assert_eq!(on.trim(), "feature");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert!(gone);
    assert_eq!(back_on.trim(), "main");
    for (i, r) in refused.iter().enumerate() {
        assert!(r.is_err(), "refusal {i}");
    }
    assert!(renamed.is_ok_and(|r| r.is_ok()));
    assert!(other);
}

#[test]
fn a_stash_is_undone_by_popping_that_stash() {
    let dir = repo("stash");
    let clean = plan(
        &dir,
        &WriteOp::Stash {
            message: "x".into(),
        },
    );
    write(&dir, "a.txt", "dirty\n");
    let p = planned(
        &dir,
        WriteOp::Stash {
            message: "wip".into(),
        },
    );
    let entry = run(&dir, &p);
    let stashed = git_out(&dir, &["status", "--porcelain"]);
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(clean.is_err());
    assert_eq!(stashed, "");
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert_eq!(back, "dirty\n");
}
