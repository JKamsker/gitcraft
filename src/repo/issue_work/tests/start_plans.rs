// Included in the parent test module to preserve qualified test names.

#[test]
fn work_starts_from_the_remotes_default_branch_as_it_is_now() {
    let dir = cloned("remote-head");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let base = tab.issue_base();
    let plan = tab.issue_plan(21, "21-x", false);
    let said = note(&plan);
    start(&mut tab, &ctx, issue(21, "X"), "21-x", false);
    let at = git_out(&dir, &["rev-parse", "21-x"]);
    let remote_dev = git_out(
        Path::new(&format!("{}-remote", dir.display())),
        &["rev-parse", "dev"],
    );
    let master = git_out(&dir, &["rev-parse", "origin/master"]);
    let current = git_out(&dir, &["branch", "--show-current"]);
    let status = tab.status.as_deref().cloned();
    let pr_base = tab.issue_pull_request("21-x").map(|(_, _, base)| base);
    clean(&dir);
    assert_eq!((base.name.as_str(), base.known), ("dev", true));
    assert!(
        said.contains("origin/dev (the remote's default branch), fetched first"),
        "{said}"
    );
    // From dev as the remote has it now (fetched first), not master, nor dev as last fetched.
    assert_eq!(at, remote_dev);
    assert_ne!(at, master);
    assert_eq!(current, "21-x");
    assert_eq!(status, Some(("Start work on #21: done".to_owned(), false)));
    // Its pull request goes into the branch it started from.
    assert_eq!(pr_base.as_deref(), Some("dev"));
}

#[test]
fn offline_it_starts_from_the_branch_as_last_fetched_and_says_so() {
    let dir = cloned("offline");
    let gone = format!("{}-gone", dir.display());
    assert!(git::run(Some(&dir), &["remote", "set-url", "origin", &gone]).is_ok());
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    start(&mut tab, &ctx, issue(22, "Y"), "22-y", false);
    let at = git_out(&dir, &["rev-parse", "22-y"]);
    let fetched = git_out(&dir, &["rev-parse", "origin/dev"]);
    let status = tab.status.as_deref().cloned().unwrap_or_default();
    let links = link::read(&dir).unwrap_or_default();
    clean(&dir);
    assert_eq!(at, fetched);
    assert!(
        status.0.contains(
            "done, but couldn't fetch origin, so it starts from origin/dev as last fetched"
        ),
        "{status:?}"
    );
    assert_eq!(links.get("22-y").map(|l| l.number), Some(22));
}

#[test]
fn a_branch_only_the_remote_has_is_tracked() {
    let dir = cloned("tracked");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let said = note(&tab.issue_plan(8, "8-elsewhere", false));
    start(&mut tab, &ctx, issue(8, "Elsewhere"), "8-elsewhere", false);
    let upstream = git_out(
        &dir,
        &["rev-parse", "--abbrev-ref", "8-elsewhere@{upstream}"],
    );
    let at = git_out(&dir, &["rev-parse", "8-elsewhere"]);
    let there = git_out(&dir, &["rev-parse", "origin/8-elsewhere"]);
    clean(&dir);
    assert!(
        said.contains("origin/8-elsewhere is there: a branch tracking it"),
        "{said}"
    );
    assert_eq!(upstream, "origin/8-elsewhere");
    assert_eq!(at, there);
}

#[test]
fn what_the_picker_says_is_what_happens() {
    let dir = cloned("plans");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    // The default branch is no branch of an issue's.
    let refused = note(&tab.issue_plan(1, "dev", false));
    assert!(
        refused.starts_with("refused: dev is the default branch"),
        "{refused}"
    );
    let bad = note(&tab.issue_plan(1, "x.lock", false));
    assert!(bad.contains(".lock"), "{bad}");
    // A branch for another issue already.
    for args in [
        &["branch", "6-taken"][..],
        &["config", "branch.6-taken.gitcraft-issue", "6"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let _ = std::fs::write(dir.join("wip.txt"), "changes\n");
    tab.reload(&ctx);
    wait(&mut tab, &ctx);
    let taken = tab.issue_plan(7, "6-taken", false);
    let worktree = tab.issue_plan(7, "6-taken", true);
    let fresh = tab.issue_plan(7, "7-fresh", true);
    clean(&dir);
    assert!(
        taken
            .warnings
            .iter()
            .any(|w| w.contains("6-taken is for #6 already")),
        "{:?}",
        taken.warnings
    );
    // Here, the changes go along; in a new worktree they stay.
    assert!(
        taken
            .warnings
            .iter()
            .any(|w| w.contains("working tree has changes")),
        "{:?}",
        taken.warnings
    );
    assert!(
        !worktree.warnings.iter().any(|w| w.contains("working tree")),
        "{:?}",
        worktree.warnings
    );
    assert!(note(&taken).contains("exists already"), "{}", note(&taken));
    assert!(
        matches!(
            &worktree.start,
            Ok(Start {
                place: Place::NewWorktree(_),
                source: Source::Exists,
                ..
            })
        ),
        "{:?}",
        worktree.start
    );
    assert!(
        note(&fresh).contains("in a new worktree"),
        "{}",
        note(&fresh)
    );
    assert!(fresh.open.is_some());
}
