// Conflicts regression coverage.

/// A repository where folding `f1.txt` into c1 conflicts with c3, which changed it since.
fn conflicting(name: &str) -> (PathBuf, String, String) {
    let dir = repo(name, 3);
    write(&dir, "f1.txt", "three\n");
    git_ok(&dir, &["commit", "-q", "-am", "c4"]);
    let before = git::head(&dir);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let was_staged = staged(&dir);
    (dir, before, was_staged)
}

/// The fold whose rebase stopped hands over to the conflict flow; aborting goes back to before
/// the fold, the change staged again (not left in a fixup! commit).
#[test]
fn a_conflict_aborted_goes_back_to_before_the_fold() {
    let (dir, before, was_staged) = conflicting("abort");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "notes.txt", "untracked\n");

    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(
        matches!(&result, Some(Err(e)) if e.contains("CONFLICT")),
        "{result:?}"
    );
    assert_eq!(tab.work.in_progress, Some(InProgress::Rebase));
    assert!(tab.fold.stopped.is_some());
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("stopped on a conflict"), "{said}");
    assert!(tab.fold_stopped_here(InProgress::Rebase).is_some());
    assert!(matches!(
        tab.abort_for(InProgress::Rebase),
        Action::AbortFold
    ));
    tab.apply(Action::AbortFold, &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let steps = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, title, .. }) => Some((title, ops)),
        _ => None,
    };
    assert!(
        steps.as_ref().is_some_and(|(_, ops)| ops.len() == 4),
        "the fold's abort"
    );
    let Some((label, ops)) = steps else { return };
    tab.apply(
        Action::Ops {
            label,
            ops,
            refresh: crate::repo::Refresh::All,
        },
        &ctx,
    );
    done(finish(&mut tab, &ctx));
    assert_eq!(git::in_progress(&dir), Ok(None));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert_eq!(read(&dir, "notes.txt"), "untracked\n");
    assert!(tab.fold.stopped.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Resolved and continued (twice: the fixup, then the later commit that changed the file),
/// the fold is one undo, back to before it with the change staged.
#[test]
fn a_conflict_continued_is_one_undo() {
    let (dir, before, was_staged) = conflicting("continue");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);

    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    let undos = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();
    for _ in 0..4 {
        if tab.work.in_progress != Some(InProgress::Rebase) {
            break;
        }
        // Each side's line made one.
        let text = read(&dir, "f1.txt");
        let resolved = if text.contains("three") {
            "fixed\n"
        } else {
            "1 fixed\n"
        };
        write(&dir, "f1.txt", resolved);
        git_ok(&dir, &["add", "f1.txt"]);
        tab.reload_status(&ctx);
        let read = tab.reads;
        wait_for(&mut tab, &ctx, "resolved status", |t| {
            t.work_read >= read && t.work.conflicted.is_empty()
        });
        tab.apply(Action::Commit, &ctx);
        assert!(
            finish(&mut tab, &ctx).is_some(),
            "continuing the rebase did not start an operation"
        );
    }
    assert_eq!(git::in_progress(&dir), Ok(None));
    assert_eq!(subjects(&dir), ["c4", "c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~3", "f1.txt"), "1 fixed\n");
    assert_eq!(at(&dir, "HEAD", "f1.txt"), "fixed\n");
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        undos + 1
    );
    assert!(tab.fold.stopped.is_none());
    let folded = git::head(&dir);

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    done(redo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), folded);
    let _ = std::fs::remove_dir_all(&dir);
}
