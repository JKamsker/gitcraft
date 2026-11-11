#[test]
fn files_validation_refuses_outside_staged_but_allows_unstaged() {
    let file = |path: &str, area| git::StatusFile {
        path: path.into(),
        old_path: None,
        code: 'M',
        area,
    };
    let mut work = WorkStatus {
        staged: vec![file("a", git::Area::Staged), file("b", git::Area::Staged)],
        unstaged: vec![file("c", git::Area::Unstaged)],
        ..WorkStatus::default()
    };
    let selected = vec!["a".to_owned()];
    assert!(files_problem(&selected, &work).is_some_and(|e| e.contains("prevents Undo")));
    assert!(files_problem(&["a".into(), "b".into()], &work).is_none());
    work.staged.pop();
    work.unstaged.push(file("b", git::Area::Unstaged));
    assert!(files_problem(&selected, &work).is_none());
    let request = StashRequest {
        mode: StashMode::Files(selected),
        message: "--".into(),
        untracked: false,
    };
    let args = push_args(&request, &work).expect("unrelated unstaged files supported");
    assert_eq!(push_request(&args).expect("decode").mode, request.mode);
}

#[test]
fn fresh_files_push_and_redo_refuse_outside_staged_paths() -> Result<(), String> {
    let repo = Repo::new("files-outside-staged")?;
    repo.write("a.txt", b"staged a\n")?;
    repo.git(&["add", "a.txt"])?;
    repo.write("old.txt", b"unrelated unstaged\n")?;
    let request = StashRequest {
        mode: StashMode::Files(vec!["a.txt".into()]),
        message: String::new(),
        untracked: false,
    };
    let stale_args = push_args(&request, &repo.status()?)?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(request.mode.clone(), false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    // The same selection becomes unsupported after another file is staged.
    repo.write("sp ace.txt", b"staged b\n")?;
    repo.git(&["add", "sp ace.txt"])?;
    let before = repo.state()?;
    let stash_before = repo.git(&["stash", "list"])?;
    assert!(push_args(&request, &repo.status()?).is_err_and(|e| e.contains("prevents Undo")));
    let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("prevents Undo")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("prevents Undo")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.git(&["stash", "list"])?, stash_before);
    // Selecting every staged path still round-trips the index and unrelated unstaged bytes.
    let (result, kind) = repo.stash(
        StashMode::Files(vec!["a.txt".into(), "sp ace.txt".into()]),
        false,
    );
    result?;
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    let redone = repo.redo(undone)?;
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn fresh_files_push_and_redo_revalidate_selected_deletions_and_renames() -> Result<(), String> {
    for renamed in [false, true] {
        let repo = Repo::new(if renamed {
            "fresh-selected-rename"
        } else {
            "fresh-selected-deletion"
        })?;
        let selected = if renamed { "picked.txt" } else { "a.txt" };
        repo.write(selected, b"selected modification\n")?;
        let request = StashRequest {
            mode: StashMode::Files(vec![selected.into()]),
            message: String::new(),
            untracked: renamed,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(request.mode.clone(), renamed);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        if renamed {
            std::fs::remove_file(repo.path().join(selected)).map_err(|e| e.to_string())?;
            repo.git(&["mv", "old.txt", selected])?;
        } else {
            repo.git(&["rm", "-f", selected])?;
        }
        let work = repo.status()?;
        assert!(
            work.staged
                .iter()
                .any(|row| row.path == selected && row.code == if renamed { 'R' } else { 'D' })
        );
        let expected =
            files_problem(&[selected.into()], &work).ok_or("selected row must be refused")?;
        assert_eq!(
            validate_push(repo.path(), &stale_args),
            Err(expected.clone())
        );
        let before = repo.state()?;
        let stashes = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
        assert_eq!(result, Err(expected.clone()));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo.clone(),
            OpKind::Redo(undone),
            None,
            false,
            None,
        );
        assert_eq!(result, Err(expected));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
    }
    Ok(())
}

#[test]
fn newly_untracked_selection_requires_reprepare_before_push_and_redo() -> Result<(), String> {
    let repo = Repo::new("files-now-untracked")?;
    repo.write("new.txt", b"selected new bytes\n")?;
    repo.git(&["add", "new.txt"])?;
    let request = StashRequest {
        mode: StashMode::Files(vec!["new.txt".into()]),
        message: String::new(),
        untracked: false,
    };
    let args = push_args(&request, &repo.status()?)?;
    assert!(!args.iter().any(|a| a == "--include-untracked"));
    let (result, kind) = repo.stash(request.mode.clone(), false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    repo.git(&["reset", "HEAD", "--", "new.txt"])?;
    assert!(has_untracked(&["new.txt".into()], &repo.status()?));
    let before = repo.state()?;
    let bytes = std::fs::read(repo.path().join("new.txt")).map_err(|e| e.to_string())?;
    let stashes = repo.git(&["stash", "list"])?;
    assert!(validate_push(repo.path(), &args).is_err_and(|e| e.contains("Open Stash again")));
    let (result, _) = run_push(repo.path(), args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("Open Stash again")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("Open Stash again")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(
        std::fs::read(repo.path().join("new.txt")).map_err(|e| e.to_string())?,
        bytes
    );
    assert_eq!(repo.git(&["stash", "list"])?, stashes);
    // Explicit preparation now includes -u for precisely the selected paths.
    assert!(
        push_args(&request, &repo.status()?)?
            .iter()
            .any(|a| a == "--include-untracked")
    );
    Ok(())
}

#[test]
fn shared_native_cleanup_guard_checks_excluded_replacements_and_failures() -> Result<(), String> {
    let removed = |path: &str, code, old: Option<&str>, area| git::StatusFile {
        path: path.into(),
        old_path: old.map(str::to_owned),
        code,
        area,
    };
    for row in [
        removed("private", 'D', None, git::Area::Staged),
        removed("renamed", 'R', Some("private"), git::Area::Staged),
        removed("private", 'D', None, git::Area::Unstaged),
    ] {
        for replacement in ["private", "private/content", "private/"] {
            let work = WorkStatus {
                staged: vec![row.clone()],
                ..Default::default()
            };
            let calls = std::cell::Cell::new(0);
            let result = validate_excluded_cleanup_with(
                &work,
                false,
                || Ok(format!("{replacement}\0")),
                |_, _, _| {
                    calls.set(calls.get() + 1);
                    Err("unexpected filesystem inspection".into())
                },
            );
            assert_eq!(calls.get(), 0);
            assert!(result.is_err(), "{replacement}");
        }
    }
    let work = WorkStatus {
        staged: vec![removed("private", 'D', None, git::Area::Staged)],
        ..Default::default()
    };
    assert!(
        validate_excluded_cleanup_with(
            &work,
            false,
            || Err("inspection failed".into()),
            |_, _, _| Ok(())
        )
        .is_err()
    );
    assert!(
        validate_excluded_cleanup_with(
            &work,
            false,
            || Ok("PRIVATE/content\0".into()),
            |_, _, paths| {
                assert_eq!(paths, &["PRIVATE/content"]);
                Err("filesystem alias".into())
            }
        )
        .is_err()
    );
    validate_excluded_cleanup_with(
        &work,
        false,
        || Ok("private-other/content\0".into()),
        |seen, untracked, paths| {
            assert_eq!(seen.staged[0].path, "private");
            assert!(!untracked);
            assert_eq!(paths, &["private-other/content"]);
            Ok(())
        },
    )?;
    Ok(())
}
