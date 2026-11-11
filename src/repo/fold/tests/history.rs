// History regression coverage.

/// Folding into the commit before HEAD: that commit gets the change, every commit keeps its
/// message, nothing stays staged, and one undo puts the branch and the staged change back.
#[test]
fn folds_into_the_commit_before_head_and_undoes_in_one_step() {
    let dir = repo("head-1", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);
    let was_staged = staged(&dir);
    let undos = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    let folded = git::head(&dir);
    assert_eq!(subjects(&dir), ["c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    assert_eq!(at(&dir, "HEAD~2", "f1.txt"), "1\n");
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD~2"]).trim(), {
        git_ok(&dir, &["rev-parse", &format!("{before}~2")])
            .trim()
            .to_owned()
    });
    assert!(staged(&dir).is_empty());
    assert_eq!(git_ok(&dir, &["status", "--porcelain"]), "");
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        undos + 1
    );

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert_eq!(read(&dir, "f2.txt"), "2 fixed\n");

    done(redo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), folded);
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Five commits back, with the four after it replayed unchanged.
#[test]
fn folds_into_a_commit_five_back() {
    let dir = repo("head-5", 6);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~5", false));
    assert_eq!(subjects(&dir), ["c6", "c5", "c4", "c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~5", "f1.txt"), "1 fixed\n");
    for (rev, file, text) in [("HEAD", "f6.txt", "6\n"), ("HEAD~2", "f4.txt", "4\n")] {
        assert_eq!(at(&dir, rev, file), text);
    }
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The first commit, which has no parent to rebase from (`--root`).
#[test]
fn folds_into_the_root_commit() {
    let dir = repo("root", 2);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let root = git_ok(&dir, &["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_owned();
    write(&dir, "root.txt", "root fixed\n");
    git_ok(&dir, &["add", "root.txt"]);

    let found = plan(&dir, &root);
    assert!(found.as_ref().is_ok_and(|p| p.root && p.after.len() == 2));
    done(fold(&mut tab, &ctx, &root, false));
    assert_eq!(subjects(&dir), ["c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~2", "root.txt"), "root fixed\n");
    let roots = git_ok(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(roots.lines().count(), 1);
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// What isn't staged stays as it was: a change to another file, a second change to the file
/// folded (only its staged part goes in), and a new file git doesn't track.
#[test]
fn unstaged_and_untracked_changes_survive() {
    let dir = repo("keeps", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    write(&dir, "f1.txt", "1 fixed\nand more\n");
    write(&dir, "f3.txt", "3 edited\n");
    write(&dir, "new.txt", "untracked\n");

    done(fold(&mut tab, &ctx, "HEAD~2", false));
    assert_eq!(at(&dir, "HEAD~2", "f1.txt"), "1 fixed\n");
    assert_eq!(at(&dir, "HEAD", "f3.txt"), "3\n");
    assert_eq!(read(&dir, "f1.txt"), "1 fixed\nand more\n");
    assert_eq!(read(&dir, "f3.txt"), "3 edited\n");
    assert_eq!(read(&dir, "new.txt"), "untracked\n");
    assert!(staged(&dir).is_empty());
    let status = git_ok(&dir, &["status", "--porcelain"]);
    assert!(status.contains(" M f1.txt") && status.contains(" M f3.txt"));
    assert!(status.contains("?? new.txt"));
    // No stash left behind by the rebase.
    assert_eq!(git_ok(&dir, &["stash", "list"]), "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Kept as a fixup! commit: nothing is rewritten, the commit is on top, undo stages it again.
#[test]
fn keeps_a_fixup_commit_for_later() {
    let dir = repo("keep", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);
    let was_staged = staged(&dir);

    done(fold(&mut tab, &ctx, "HEAD~1", true));
    assert_eq!(subjects(&dir), ["fixup! c2", "c3", "c2", "c1", "root"]);
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD~1"]).trim(), before);
    assert!(staged(&dir).is_empty());

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    let _ = std::fs::remove_dir_all(&dir);
}
