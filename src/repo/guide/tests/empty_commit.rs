// Empty commit regression coverage.

#[test]
fn the_empty_first_commit_is_undone_back_to_no_commits_and_redone() {
    let dir = empty_repo("empty-commit");
    let (made, entry) = make_empty_commit(&dir);
    let head = git::head(&dir);
    let entry = entry.unwrap_or_else(|| UndoEntry {
        id: 0,
        agent: None,
        label: String::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    });
    let undone = undo(&dir, &entry);
    let after_undo = (
        git::head(&dir),
        git::refs(&dir).unwrap_or_default().current,
        steps(&dir),
    );
    let redone = redo(&dir, &entry);
    let after_redo = git::head(&dir);
    let message = git::head_message(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok(), "{made:?}");
    assert!(!head.is_empty());
    assert!(undone.is_ok(), "{undone:?}");
    assert_eq!(after_undo.0, "");
    assert_eq!(after_undo.1, None);
    assert!(after_undo.2.is_some_and(|s| s.empty && s.nothing_to_commit));
    assert!(redone.is_ok(), "{redone:?}");
    assert!(!after_redo.is_empty());
    assert_eq!(message.unwrap_or_default().trim(), "Initial commit");
}

#[test]
fn undoing_the_empty_commit_refuses_once_more_was_committed() {
    let dir = empty_repo("empty-commit-moved");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "more"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert!(!head.is_empty());
}

/// Empty commit, undo, then a real first commit: redoing the empty one must not go on top
/// of it (and then undoing that must not delete the branch with the real commit).
#[test]
fn redoing_the_empty_commit_after_a_real_first_commit_is_refused() {
    let dir = empty_repo("redo-after-real");
    let (_, entry) = make_empty_commit(&dir);
    let undone = entry.as_ref().map(|e| undo(&dir, e));
    assert!(std::fs::write(dir.join("real work.txt"), "x\n").is_ok());
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "real"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let real = git::head(&dir);
    let redone = entry.as_ref().map(|e| redo(&dir, e));
    let undone_again = entry.as_ref().map(|e| undo(&dir, e));
    let after = git::head(&dir);
    let count = git::run(Some(&dir), &["rev-list", "--count", "HEAD"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.is_some_and(|r| r.is_ok()));
    assert!(redone.as_ref().is_some_and(|r| r.is_err()), "{redone:?}");
    assert!(redone.is_some_and(|r| r.is_err_and(|e| e.contains("has commits now"))));
    assert!(undone_again.is_some_and(|r| r.is_err()));
    assert_eq!(after, real);
    assert_eq!(count.trim(), "1");
}

/// The commit itself is refused when HEAD got a commit while the dialog was open.
#[test]
fn the_empty_commit_is_refused_once_the_branch_has_a_commit() {
    let dir = empty_repo("commit-meanwhile");
    let (commit, how) = empty_commit_steps("refs/heads/main");
    assert!(
        git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "meanwhile"]
        )
        .is_ok()
    );
    let (made, _) = run_steps(&dir, &commit, OpKind::Normal(how, String::new()), None);
    let count = git::run(Some(&dir), &["rev-list", "--count", "HEAD"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_err());
    assert_eq!(count.trim(), "1");
}

/// Undoing with HEAD detached would have removed `.git/HEAD` (no repository any more).
#[test]
fn undoing_the_empty_commit_with_a_detached_head_is_refused() {
    let dir = empty_repo("undo-detached");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["checkout", "-q", "--detach"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let still_a_repo = git::run(Some(&dir), &["rev-parse", "--git-dir"]).is_ok();
    let head = git::head(&dir);
    let main = branch_exists(&dir, "refs/heads/main");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert!(still_a_repo);
    assert!(!head.is_empty());
    assert!(main);
}

/// With another branch made at the same commit and checked out, undoing would have deleted
/// that branch instead.
#[test]
fn undoing_the_empty_commit_after_switching_to_a_new_branch_is_refused() {
    let dir = empty_repo("undo-switched");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "feature-ü"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let both = (
        branch_exists(&dir, "refs/heads/main"),
        branch_exists(&dir, "refs/heads/feature-ü"),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert_eq!(both, (true, true));
}

/// Once published, Ctrl+Z on the empty first commit would delete the branch the remote has.
#[test]
fn undoing_the_empty_commit_after_publishing_it_is_refused() {
    let ctx = egui::Context::default();
    let dir = empty_repo("undo-published");
    let bare = bare_beside(&dir);
    let (_, entry) = make_empty_commit(&dir);
    for args in [
        &["remote", "add", "origin", &bare.to_string_lossy()][..],
        &["push", "-q", "-u", "origin", "refs/heads/main"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let mut tab = read_tab(&ctx, &dir, false);
    if let Some(entry) = entry {
        tab.remember_undo(&entry.label, entry.clone(), &ctx);
    }
    tab.apply(Action::Undo, &ctx);
    let main = branch_exists(&dir, "refs/heads/main");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        1
    );
    assert!(tab.busy_op.is_none());
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(m, err)| *err && m.contains("main is published"))
    );
    assert!(main);
}

/// A new first commit starts a new timeline branch; the old redo remains available in the tree.
#[test]
fn a_first_commit_does_not_redo_an_old_empty_commit_on_top() {
    let ctx = egui::Context::default();
    let dir = empty_repo("redo-tree");
    let mut tab = read_tab(&ctx, &dir, false);
    tab.timeline.loaded = true;
    tab.timeline_read(0, &ctx);
    let entry = UndoEntry {
        id: 0,
        agent: None,
        label: EMPTY_COMMIT_LABEL.to_owned(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    };
    tab.remember_undo(&entry.label, entry.clone(), &ctx);
    let old = tab.timeline.current;
    tab.timeline.move_to(None, Some(entry));
    assert!(tab.timeline.redo_target().is_some());
    assert!(
        git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "real first"]
        )
        .is_ok()
    );
    tab.refs = git::refs(&dir).unwrap_or_default();
    tab.timeline_read(1, &ctx);
    assert_ne!(tab.timeline.current, old);
    assert!(tab.timeline.redo_target().is_none());
    assert!(
        old.is_some_and(|id| tab.timeline.get(id).is_some()),
        "the old redo stays in the undo tree"
    );
    let _ = std::fs::remove_dir_all(dir);
}
