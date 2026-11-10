// Collision guards regression coverage.

/// A fold stopped, then aborted in the terminal and another branch's rebase stopped in its
/// place: Abort is the ordinary one for that rebase, and resets no branch to the fold's start.
#[test]
fn abort_is_the_folds_only_while_its_rebase_is_going_on() {
    let (dir, before, _) = conflicting("abort-other");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    git_ok(&dir, &["rebase", "--abort"]);
    git_ok(&dir, &["reset", "-q", "--hard", &before]);
    // Another branch, whose rebase stops on a conflict too.
    git_ok(&dir, &["checkout", "-q", "-b", "other", "HEAD~1"]);
    write(&dir, "f1.txt", "other\n");
    git_ok(&dir, &["commit", "-q", "-am", "other"]);
    let other = git::head(&dir);
    let _ = git::run(Some(&dir), &["rebase", "main"]);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    tab.reload(&ctx);
    settle(&mut tab, &ctx);

    tab.apply(tab.abort_for(InProgress::Rebase), &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let ops = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, .. }) => ops,
        _ => Vec::new(),
    };
    assert_eq!(ops, crate::repo::ops(&[&["rebase", "--abort"]]));
    assert_eq!(git::resolve(&dir, "refs/heads/other"), Ok(other));
    let _ = git::run(Some(&dir), &["rebase", "--abort"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An ignored file where a commit to replay has a file would be written over by the rebase
/// (git doesn't ask for ignored files): the fold is refused before anything runs.
#[test]
fn an_ignored_file_in_the_way_is_refused() {
    let dir = repo("ignored", 1);
    write(&dir, "build.log", "tracked once\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-q", "-m", "log"]);
    git_ok(&dir, &["rm", "-q", "--cached", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-q", "-m", "ignore it"]);
    write(&dir, "build.log", "mine, not to lose\n");
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let refused = plan(&dir, "HEAD~2").err().unwrap_or_default();
    assert!(refused.contains("build.log"), "{refused}");
    assert_eq!(read(&dir, "build.log"), "mine, not to lose\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A merge started after the dialog read the branch (the same branch, the same HEAD): the
/// fold refuses rather than commit the merge as its fixup.
#[test]
fn a_merge_started_since_the_dialog_stops_the_fold() {
    let dir = repo("merge-since", 2);
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    write(&dir, "side.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~1");
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    // git merges only with nothing staged: the change staged again once it has started.
    git_ok(&dir, &["reset", "-q", "f1.txt"]);
    git_ok(&dir, &["merge", "-q", "--no-ff", "--no-commit", "side"]);
    git_ok(&dir, &["add", "f1.txt"]);
    let before = git::head(&dir);
    tab.apply(Action::RunFold(Box::new(found.clone())), &ctx);
    settle(&mut tab, &ctx);
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("merge is in progress"), "{said}");
    assert_eq!(git::head(&dir), before);
    // And should it start between that check and the commit, the steps' own checks refuse.
    let Ok(steps) = found.editor().and_then(|e| e.command(&dir)) else {
        return;
    };
    tab.fold_ready(&found, Ok((steps, String::new())), &ctx);
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(git::head(&dir), before);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Merge)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `git am` stopped after the dialog read the branch (no ref says so, only its folder): the
/// fold sees it right before it runs, and commits nothing.
#[test]
fn a_patch_stopped_since_the_dialog_stops_the_fold() {
    let dir = repo("am-since", 2);
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    write(&dir, "f2.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side"]);
    let patch = git_ok(&dir, &["format-patch", "-1", "--stdout"]);
    write(&dir, ".git/side.patch", &patch);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~1");
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    // f2.txt differs on main: the patch stops. Staged meanwhile, the fix stays staged.
    git_ok(&dir, &["reset", "-q", "f1.txt"]);
    let stopped = git::run(Some(&dir), &["am", "-q", ".git/side.patch"]);
    git_ok(&dir, &["add", "f1.txt"]);
    assert!(stopped.is_err());
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Am)));
    let before = git::head(&dir);
    tab.apply(Action::RunFold(Box::new(found)), &ctx);
    assert!(
        tab.busy_op.is_some(),
        "the preflight check holds the operation busy"
    );
    settle(&mut tab, &ctx);
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("patch is in progress"), "{said}");
    assert_eq!(git::head(&dir), before);
    let _ = git::run(Some(&dir), &["am", "--abort"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The fold's abort asked for, then (while its dialog was open) the rebase replaced in the
/// terminal by one of another branch: confirming refuses, and that rebase goes on untouched.
#[test]
fn an_abort_confirmed_after_the_rebase_was_replaced_resets_no_other_branch() {
    let (dir, before, _) = conflicting("abort-replaced");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    tab.apply(Action::AbortFold, &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let asked = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, title, .. }) => Some((title, ops)),
        _ => None,
    };
    assert!(asked.is_some());
    let Some((label, ops)) = asked else { return };
    // Meanwhile, in the terminal: the fold's rebase aborted, another branch's started.
    git_ok(&dir, &["rebase", "--abort"]);
    git_ok(&dir, &["checkout", "-q", "-b", "other", &before]);
    write(&dir, "f1.txt", "other\n");
    git_ok(&dir, &["commit", "-q", "-am", "other"]);
    let other = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "-b", "third", "HEAD~1"]);
    write(&dir, "f1.txt", "third\n");
    git_ok(&dir, &["commit", "-q", "-am", "third"]);
    let _ = git::run(Some(&dir), &["rebase", "other"]);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    let third = git::resolve(&dir, "refs/heads/third");

    tab.apply(
        Action::Ops {
            label,
            ops,
            refresh: crate::repo::Refresh::All,
        },
        &ctx,
    );
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    // The other rebase goes on, its branches as they were.
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    assert_eq!(
        git::rebasing_branch(&dir),
        Ok(Some("refs/heads/third".to_owned()))
    );
    assert_eq!(git::resolve(&dir, "refs/heads/third"), third);
    assert_eq!(git::resolve(&dir, "refs/heads/other"), Ok(other));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same for an ignored file whose name has spaces around it, and one where a commit to
/// replay renamed a file away from (its old name is written when the commit before is).
#[test]
fn ignored_files_named_with_spaces_or_renamed_away_are_refused() {
    // Windows takes trailing spaces off file names: such a file can't be there.
    let cases: &[(&str, bool)] = if cfg!(windows) {
        &[("old-name", true)]
    } else {
        &[(" collision ", false), ("old-name", true)]
    };
    for &(name, rename) in cases {
        let dir = repo(
            if rename {
                "ignored-renamed"
            } else {
                "ignored-spaces"
            },
            1,
        );
        write(&dir, name, "tracked once\n");
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", "add it"]);
        if rename {
            git_ok(&dir, &["mv", name, "new-name"]);
            git_ok(&dir, &["commit", "-q", "-m", "rename it"]);
            git_ok(&dir, &["rm", "-q", "new-name"]);
        } else {
            git_ok(&dir, &["rm", "-q", name]);
        }
        git_ok(&dir, &["commit", "-q", "-m", "remove it"]);
        let exclude = dir.join(".git").join("info").join("exclude");
        // Spaces escaped: a pattern's trailing ones would be dropped.
        let pattern = name.replace(' ', "\\ ");
        assert!(std::fs::write(&exclude, format!("{pattern}\n")).is_ok());
        write(&dir, name, "mine, not to lose\n");
        write(&dir, "f1.txt", "1 fixed\n");
        git_ok(&dir, &["add", "f1.txt"]);
        let target = if rename { "HEAD~1" } else { "HEAD~2" };
        let refused = plan(&dir, target).err().unwrap_or_default();
        assert!(refused.contains(name), "{name:?}: {refused}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn ignored_file_created_after_dialog_is_refused_by_the_mutation_worker() -> Result<(), String> {
    let dir = repo("ignored-since-dialog", 1);
    write(&dir, "build.log", "tracked once\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-qm", "log"]);
    git_ok(&dir, &["rm", "-q", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-qm", "ignore"]);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2")?;
    let before = git::head(&dir);
    write(&dir, "build.log", "private ignored content\n");
    let (steps, _) = found.fold_steps(vec!["--must-never-run".into()]);
    let (result, _) =
        crate::repo::operations::run_steps(&dir, &steps, crate::repo::OpKind::plain(), None);
    assert!(result.is_err());
    assert!(result.err().unwrap_or_default().contains("build.log"));
    assert_eq!(git::head(&dir), before);
    assert_eq!(read(&dir, "build.log"), "private ignored content\n");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn staged_deletion_is_included_in_fold_collision_paths() -> Result<(), String> {
    let dir = repo("staged-delete-collision", 2);
    write(&dir, ".git/info/exclude", "root.txt\n");
    git_ok(&dir, &["rm", "--cached", "root.txt"]);
    let read = git::fold_read(&dir, "HEAD~1")?;
    assert_eq!(read.ignored_in_way.as_deref(), Some("root.txt"));
    assert!(Plan::from_read(read).is_err());
    assert_eq!(self::read(&dir, "root.txt"), "root\n");
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(unix)]
#[test]
fn fixup_hook_ignored_collision_stops_before_rebase() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = repo("hook-ignored-collision", 1);
    write(&dir, "build.log", "historical\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-qm", "historical log"]);
    git_ok(&dir, &["rm", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-qm", "ignore log"]);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2")?;
    let before = git::head(&dir);
    write(
        &dir,
        ".git/hooks/pre-commit",
        "#!/bin/sh\nprintf 'private hook output\\n' > build.log\n",
    );
    std::fs::set_permissions(
        dir.join(".git/hooks/pre-commit"),
        std::fs::Permissions::from_mode(0o700),
    )
    .map_err(|e| e.to_string())?;
    let (steps, _) = found.fold_steps(vec!["--must-never-run".into()]);
    let (result, _) =
        crate::repo::operations::run_steps(&dir, &steps, crate::repo::OpKind::plain(), None);
    assert!(result.err().unwrap_or_default().contains("build.log"));
    assert_ne!(git::head(&dir), before);
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD^"]).trim(), before);
    assert_eq!(read(&dir, "build.log"), "private hook output\n");
    assert!(git::in_progress(&dir)?.is_none());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
