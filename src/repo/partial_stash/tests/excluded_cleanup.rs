#[test]
fn excluded_overlap_validation_handles_ancestry_and_message_options() {
    for (deleted, untracked, blocked) in [
        ("f", "f", true),
        ("f", "f/child", true),
        ("dir/f", "dir", true),
        ("dir/f", "dir/", true),
        ("dir/f", "dir///", true),
        ("dir/", "dir/f", true),
        ("dir/", "dir///", true),
        ("f", "foo", false),
        ("dir/", "directory/f", false),
        ("dir/f", "dir/other", false),
    ] {
        let work = WorkStatus {
            staged: vec![git::StatusFile {
                path: deleted.into(),
                old_path: None,
                code: 'D',
                area: git::Area::Staged,
            }],
            unstaged: vec![git::StatusFile {
                path: untracked.into(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            }],
            ..WorkStatus::default()
        };
        assert_eq!(push_everything_problem(&work, false).is_some(), blocked);
        assert!(push_everything_problem(&work, true).is_none());
        assert!(everything_problem(&work, false).is_none()); // Snapshot remains eligible.
        let request = StashRequest {
            mode: StashMode::Everything,
            message: "--include-untracked".into(),
            untracked: false,
        };
        assert_eq!(push_args(&request, &work).is_err(), blocked);
    }
    for message in ["--include-untracked", "--", "--staged"] {
        let args = ops(&[&["stash", "push", "-m", message]]).remove(0);
        let request = push_request(&args).expect("stash push");
        assert_eq!(request.mode, StashMode::Everything);
        assert!(!request.untracked);
    }
}

#[test]
fn fresh_push_and_redo_refuse_recreated_deleted_file() -> Result<(), String> {
    let repo = Repo::new("excluded-overlap-fresh")?;
    repo.write("a.txt", b"changed\n")?;
    let request = StashRequest {
        mode: StashMode::Everything,
        message: String::new(),
        untracked: false,
    };
    let stale_args = push_args(&request, &repo.status()?)?;
    let (result, kind) = repo.stash(StashMode::Everything, false);
    result?;
    let undone = repo.undo(entry(kind)?)?;
    repo.git(&["rm", "-f", "a.txt"])?;
    repo.write("a.txt", b"excluded new bytes\n")?;
    let before = repo.state()?;
    let stash_before = repo.git(&["stash", "list"])?;
    let (result, _) = run_push(repo.path(), stale_args, git::head(repo.path()), false);
    assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
    let (result, _, stopped) = run_steps_counted(
        repo.path(),
        &undone.redo.clone(),
        OpKind::Redo(undone),
        None,
        false,
        None,
    );
    assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
    assert_eq!(stopped, 0);
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.git(&["stash", "list"])?, stash_before);
    assert!(push_args(&request, &repo.status()?).is_err());
    // Saving a snapshot does not run destructive stash cleanup.
    let (result, _) = run_snapshot(repo.path(), "safe snapshot", false, git::head(repo.path()));
    result?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn rename_source_validation_preserves_git_path_spelling() {
    for (source, other, blocked) in [
        ("src", "src/child", true),
        ("src/child", "src", true),
        ("src/child", "src/", true),
        ("src/", "src/child", true),
        ("A", "a", false),
        ("dir", "directory/f", false),
        ("a\\b", "a/b", false),
        (" f ", "f", false),
    ] {
        let work = WorkStatus {
            staged: vec![git::StatusFile {
                path: "destination".into(),
                old_path: Some(source.into()),
                code: 'R',
                area: git::Area::Staged,
            }],
            unstaged: vec![git::StatusFile {
                path: other.into(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            }],
            ..WorkStatus::default()
        };
        assert_eq!(excluded_overlap_problem(&work, false).is_some(), blocked);
        assert!(everything_problem(&work, false).is_none());
    }
}

#[test]
fn embedded_repository_replacements_refuse_fresh_push_and_redo() -> Result<(), String> {
    for ignored in [false, true] {
        let repo = Repo::new(if ignored {
            "ignored-embedded-overlap"
        } else {
            "embedded-overlap"
        })?;
        std::fs::create_dir(repo.path().join("dir")).map_err(|e| e.to_string())?;
        repo.write("dir/file", b"tracked contents\n")?;
        repo.git(&["add", "dir/file"])?;
        repo.git(&["commit", "-qm", "tracked directory"])?;
        repo.write("a.txt", b"eligible queued change\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked: false,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, false);
        result?;
        let undone = repo.undo(entry(kind)?)?;

        repo.git(&["rm", "-r", "dir"])?;
        std::fs::create_dir(repo.path().join("dir")).map_err(|e| e.to_string())?;
        git::run(Some(&repo.path().join("dir")), &["init", "-q"])?;
        repo.write("dir/file", b"replacement must survive\n")?;
        if ignored {
            std::fs::write(repo.path().join(".git/info/exclude"), "/dir/\n")
                .map_err(|e| e.to_string())?;
            let excluded = repo.git(&[
                "ls-files",
                "--others",
                "--ignored",
                "--exclude-standard",
                "-z",
            ])?;
            assert!(excluded.split('\0').any(|p| p == "dir/"));
        }
        let work = repo.status()?;
        assert_eq!(
            work.unstaged
                .iter()
                .any(|row| row.area == git::Area::Untracked && row.path == "dir/"),
            !ignored,
        );
        if !ignored {
            assert!(push_args(&request, &work).is_err());
        }
        assert!(everything_problem(&work, false).is_none());
        let before = repo.state()?;
        let stashes = repo.git(&["stash", "list"])?;
        let head = git::head(repo.path());
        assert!(validate_push(repo.path(), &stale_args).is_err());
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|error| error.contains("dir/")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo.clone(),
            OpKind::Redo(undone),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|error| error.contains("dir/")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
        assert_eq!(git::head(repo.path()), head);
    }
    Ok(())
}

#[test]
fn ignored_replacements_refuse_shared_agent_stash_execution() -> Result<(), String> {
    for case in ["file", "file-to-dir", "dir-to-file", "rename"] {
        let repo = Repo::new(&format!("ignored-overlap-{case}"))?;
        let replacement = match case {
            "dir-to-file" => {
                std::fs::create_dir(repo.path().join("folder")).map_err(|e| e.to_string())?;
                repo.write("folder/child", b"tracked\n")?;
                repo.git(&["add", "folder"])?;
                repo.git(&["commit", "-qm", "directory"])?;
                repo.git(&["rm", "-r", "folder"])?;
                repo.write("folder", b"excluded file\n")?;
                "folder"
            }
            "rename" => {
                repo.git(&["mv", "old.txt", "renamed.txt"])?;
                repo.write("old.txt", b"excluded rename source\n")?;
                "old.txt"
            }
            _ => {
                repo.git(&["rm", "del.txt"])?;
                if case == "file-to-dir" {
                    std::fs::create_dir(repo.path().join("del.txt")).map_err(|e| e.to_string())?;
                    repo.write("del.txt/child", b"excluded child\n")?;
                } else {
                    repo.write("del.txt", b"excluded replacement\n")?;
                }
                "del.txt"
            }
        };
        std::fs::write(
            repo.path().join(".git/info/exclude"),
            format!("/{replacement}\n"),
        )
        .map_err(|e| e.to_string())?;
        assert!(
            !repo
                .status()?
                .unstaged
                .iter()
                .any(|f| f.area == git::Area::Untracked)
        );
        let before = repo.state()?;
        let stash_before = repo.git(&["stash", "list"])?;
        // The agent plan uses these shared arguments and the shared execution runner.
        let args = ops(&[&stash_push_args()]);
        let (result, _, stopped) =
            run_steps_counted(repo.path(), &args, OpKind::plain(), None, false, None);
        assert!(
            result.is_err_and(|e| e.contains("overlaps a deletion")),
            "{case}"
        );
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(repo.git(&["stash", "list"])?, stash_before);
        let (result, _) = run_snapshot(repo.path(), "snapshot", false, git::head(repo.path()));
        result?;
        assert_eq!(repo.state()?, before);
    }
    Ok(())
}
