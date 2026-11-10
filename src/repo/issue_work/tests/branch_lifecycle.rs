// Included in the parent test module to preserve qualified test names.

#[test]
fn a_branch_named_after_the_issue_is_made_and_checked_out_with_its_link() {
    let dir = repo("branch");
    let issue = issue(42, "Fix login crash");
    let link = IssueLink::of(&issue);
    let start = Start {
        branch: branch_name(issue.number, &issue.title),
        source: new_from("refs/remotes/origin/main"),
        place: Place::Here,
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let at = git_out(&dir, &["rev-parse", "HEAD"]);
    let remote = git_out(&dir, &["rev-parse", "origin/main"]);
    let tracks = git::run(Some(&dir), &["config", "branch.42-fix-login-crash.merge"]);
    let links = link::read(&dir);
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current, "42-fix-login-crash");
    // From where the remote's default branch is, and not tracking it.
    assert_eq!(at, remote);
    assert!(tracks.is_err(), "{tracks:?}");
    let links = links.unwrap_or_default();
    let read = links.get("42-fix-login-crash");
    assert_eq!(read, Some(&link));
    assert_eq!(
        read.map(|l| l.criteria.as_str()),
        Some("- [ ] no crash\n- [x] a test")
    );
    assert_eq!(read.map(|l| l.read.len()), Some(10));
}

#[test]
fn a_worktree_named_after_the_issue_shares_the_link() {
    let dir = repo("worktree");
    let issue = issue(7, "Füße — ünïcode 日本語 🎉");
    let link = IssueLink::of(&issue);
    let name = branch_name(issue.number, &issue.title);
    let folder = worktree_dir(&dir);
    let start = Start {
        branch: name.clone(),
        source: new_from("refs/remotes/origin/main"),
        place: Place::NewWorktree(folder.to_string_lossy().into_owned()),
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let here = git_out(&dir, &["branch", "--show-current"]);
    let there = git_out(&folder, &["branch", "--show-current"]);
    // Read from the worktree, as its own tab reads them.
    let links = link::read(&folder).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(name, "7-füße-ünïcode-日本語");
    assert_eq!(here, "main");
    assert_eq!(there, name);
    assert_eq!(links.get(&name), Some(&link));
}

#[test]
fn a_branch_there_already_is_checked_out_as_it_is() {
    let dir = repo("exists");
    assert!(git::run(Some(&dir), &["branch", "3-again"]).is_ok());
    let before = git_out(&dir, &["rev-parse", "3-again"]);
    let link = IssueLink::of(&issue(3, "Again"));
    let start = Start {
        branch: "3-again".into(),
        source: Source::Exists,
        place: Place::Here,
        fetch: None,
    };
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let after = git_out(&dir, &["rev-parse", "HEAD"]);
    let links = link::read(&dir).unwrap_or_default();
    clean(&dir);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!((current.as_str(), after), ("3-again", before));
    assert_eq!(links.get("3-again").map(|l| l.number), Some(3));
}

#[test]
fn a_branch_a_deleted_worktree_held_is_checked_out_here() {
    let dir = repo("pruned");
    let folder = worktree_dir(&dir);
    let there = folder.to_string_lossy().into_owned();
    assert!(
        git::run(
            Some(&dir),
            &["worktree", "add", "-q", "-b", "4-held", &there]
        )
        .is_ok()
    );
    // Its folder deleted by hand: git still says the branch is checked out there.
    let _ = std::fs::remove_dir_all(&folder);
    let refused = git::run(Some(&dir), &["checkout", "4-held"]);
    let start = Start {
        branch: "4-held".into(),
        source: Source::Exists,
        place: Place::Here,
        fetch: None,
    };
    let link = IssueLink::of(&issue(4, "Held"));
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let current = git_out(&dir, &["branch", "--show-current"]);
    clean(&dir);
    assert!(refused.is_err(), "{refused:?}");
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current, "4-held");
}

#[test]
fn a_checkout_changes_are_in_the_way_of_leaves_no_branch_and_no_link() {
    let dir = repo("dirty");
    // `main` has a file the remote's branch changed, and it's changed here too.
    let write = |text: &str| std::fs::write(dir.join("a.txt"), text);
    assert!(write("one\n").is_ok());
    for args in [&["add", "a.txt"][..], &["commit", "-q", "-m", "a"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    assert!(write("two\n").is_ok());
    for args in [
        &["commit", "-q", "-am", "remote's a"][..],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["reset", "-q", "--hard", "HEAD~1"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    assert!(write("mine\n").is_ok());
    let start = Start {
        branch: "5-dirty".into(),
        source: new_from("refs/remotes/origin/main"),
        place: Place::Here,
        fetch: None,
    };
    let link = IssueLink::of(&issue(5, "Dirty"));
    let (result, _) = run_steps(&dir, &link::start_ops(&start, &link), OpKind::plain(), None);
    let branch = git::run(Some(&dir), &["rev-parse", "--verify", "-q", "5-dirty"]);
    let links = link::read(&dir).unwrap_or_default();
    let kept = std::fs::read_to_string(dir.join("a.txt")).unwrap_or_default();
    clean(&dir);
    assert!(result.is_err(), "{result:?}");
    assert!(branch.is_err(), "{branch:?}");
    assert!(links.is_empty(), "{links:?}");
    assert_eq!(kept, "mine\n");
}

#[test]
fn the_link_follows_the_branch_and_can_be_forgotten() {
    let dir = repo("follow");
    let link = IssueLink {
        number: 9,
        title: "-starts like an option".into(),
        criteria: "- [ ] a\n\n  - [ ] nested: yes".into(),
        read: "2026-11-06".into(),
        ..Default::default()
    };
    let steps = [
        &["branch", "9-x"][..],
        &["branch", "release.v1.2"],
        &["branch", "other"],
        &["branch", "old"],
    ];
    for args in steps {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let mut ops = link::write_ops("9-x", &link);
    ops.extend(link::write_ops("release.v1.2", &link));
    let (wrote, _) = run_steps(&dir, &ops, OpKind::plain(), None);
    // A link written before gitcraft kept what and when: its number and title alone.
    for args in [
        &["config", "branch.old.gitcraft-issue", "3"][..],
        &["config", "branch.old.gitcraft-issue-title", "Old"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let written = link::read(&dir).unwrap_or_default();
    let _ = git::run(Some(&dir), &["branch", "-m", "9-x", "9-renamed"]);
    let renamed = link::read(&dir).unwrap_or_default();
    let unlinked = link::unlink(&dir, "9-renamed");
    let old_unlinked = link::unlink(&dir, "old");
    let _ = git::run(Some(&dir), &["branch", "-D", "release.v1.2"]);
    let after = link::read(&dir);
    clean(&dir);
    assert!(wrote.is_ok(), "{wrote:?}");
    assert_eq!(written.get("9-x"), Some(&link));
    assert_eq!(written.get("release.v1.2"), Some(&link));
    assert_eq!(written.get("old").map(|l| l.number), Some(3));
    assert!(!written.contains_key("other"));
    assert_eq!(renamed.get("9-renamed"), Some(&link));
    assert!(!renamed.contains_key("9-x"));
    assert_eq!(unlinked, Ok(()));
    // The settings it never had are no error.
    assert_eq!(old_unlinked, Ok(()));
    // No branch has one any more: git says so with exit code 1, which is no error.
    assert_eq!(after.map(|l| l.len()), Ok(0));
}
