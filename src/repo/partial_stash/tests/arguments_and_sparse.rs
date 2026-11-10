#[test]
fn the_arguments_are_what_git_stash_push_takes() -> Result<(), String> {
    let work = WorkStatus {
        unstaged: vec![
            git::StatusFile {
                path: "-n.txt".to_owned(),
                old_path: None,
                code: '?',
                area: git::Area::Untracked,
            },
            git::StatusFile {
                path: "[ab].txt".to_owned(),
                old_path: None,
                code: 'M',
                area: git::Area::Unstaged,
            },
        ],
        ..WorkStatus::default()
    };
    let request = |mode, message: &str, untracked| StashRequest {
        mode,
        message: message.to_owned(),
        untracked,
    };
    let files = StashMode::Files(vec!["[ab].txt".to_owned(), "-n.txt".to_owned()]);
    assert_eq!(
        push_args(&request(files, " -m ", false), &work).map_err(|e| e.to_string())?,
        [
            "stash",
            "push",
            "--include-untracked",
            "-m",
            "-m",
            "--",
            ":(literal)-n.txt",
            ":(literal)[ab].txt"
        ]
    );
    assert_eq!(
        push_args(&request(StashMode::Everything, "", true), &work).map_err(|e| e.to_string())?,
        ["stash", "push", "--include-untracked"]
    );
    assert_eq!(
        push_args(&request(StashMode::Everything, "", false), &work).map_err(|e| e.to_string())?,
        ["stash", "push"]
    );
    assert!(push_args(&request(StashMode::Staged, "", false), &work).is_err());
    assert!(
        push_args(
            &request(StashMode::Everything, "", true),
            &WorkStatus::default()
        )
        .is_err()
    );
    assert!(push_args(&request(StashMode::Files(Vec::new()), "", true), &work).is_err());
    Ok(())
}

#[test]
fn picks_are_forgotten_once_their_files_have_no_changes() -> Result<(), String> {
    let mut picks = StashPicks::default();
    picks.toggle("a");
    picks.toggle("b");
    picks.toggle("b");
    picks.toggle("c");
    let work = WorkStatus {
        staged: vec![git::StatusFile {
            path: "c".to_owned(),
            old_path: None,
            code: 'M',
            area: git::Area::Staged,
        }],
        ..WorkStatus::default()
    };
    picks.keep_changed(&work);
    assert_eq!(picks.picked.iter().collect::<Vec<_>>(), ["c"]);
    Ok(())
}

#[test]
fn too_many_picked_files_for_one_command_line_are_refused() {
    let paths: Vec<String> = (0..400)
        .map(|i| format!("{}/{i}.txt", "d".repeat(80)))
        .collect();
    let work = WorkStatus {
        unstaged: paths
            .iter()
            .map(|p| git::StatusFile {
                path: p.clone(),
                old_path: None,
                code: 'M',
                area: git::Area::Unstaged,
            })
            .collect(),
        ..WorkStatus::default()
    };
    let problem = files_problem(&paths, &work).unwrap_or_default();
    assert!(problem.contains("too many"), "{problem}");
    assert_eq!(files_problem(&paths[..50], &work), None);
}

/// A repository inside this one is left out of a snapshot, as `git stash -u` leaves it out.
#[test]
fn a_snapshot_leaves_out_a_nested_repository() -> Result<(), String> {
    let repo = Repo::new("snapshot-nested")?;
    let nested = repo.path().join("nested");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    git::run(Some(&nested), &["init", "-q"])?;
    std::fs::write(nested.join("x.txt"), b"x\n").map_err(|e| e.to_string())?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&[
        "-c",
        "core.quotePath=false",
        "ls-tree",
        "-r",
        "--name-only",
        "stash@{0}^3",
    ])?;
    assert_eq!(stored.trim(), NEW);
    Ok(())
}

/// Stashing everything without untracked files when only those changed is refused up front.
#[test]
fn everything_without_untracked_files_needs_tracked_changes() -> Result<(), String> {
    let repo = Repo::new("everything-untracked-only")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Everything, false);
    assert!(result.is_err_and(|e| e.contains("only untracked")));
    assert_eq!(repo.stashes(), 0);
    Ok(())
}

/// Undone, more staged elsewhere, redone: the stash now holds more than the undo's patch, so the
/// undo refuses instead of putting back part of it and dropping the rest.
#[test]
fn a_redone_staged_stash_holding_more_is_not_undone_by_the_old_patch() -> Result<(), String> {
    let repo = Repo::new("staged-redo-more")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.write("a.txt", b"staged\n")?;
    repo.git(&["add", "a.txt"])?;
    let (result, kind) = repo.stash(StashMode::Staged, false);
    assert!(result.is_ok(), "{result:?}");
    let undone = repo.undo(entry(kind)?)?;
    // In the terminal meanwhile: something else staged too.
    repo.write("sp ace.txt", b"also staged\n")?;
    repo.git(&["add", "sp ace.txt"])?;
    let redone = repo.redo(undone)?;
    let (result, _) = run_steps(
        repo.path(),
        &redone.undo,
        OpKind::Undo(redone.clone()),
        None,
    );
    assert!(result.is_err(), "the old patch was applied");
    // Nothing lost: the stash holds both, and pops back whole.
    assert_eq!(repo.stashes(), 1);
    repo.git(&["stash", "pop", "-q", "--index"])?;
    assert_eq!(repo.read("sp ace.txt"), b"also staged\n");
    assert_eq!(repo.read("a.txt"), b"staged\n");
    Ok(())
}

/// In a sparse checkout, an untracked file outside the cone goes in the snapshot, as `git stash
/// --include-untracked` takes it.
#[test]
fn a_snapshot_takes_untracked_files_outside_a_sparse_cone() -> Result<(), String> {
    let repo = Repo::new("snapshot-sparse")?;
    std::fs::create_dir_all(repo.path().join("in")).map_err(|e| e.to_string())?;
    repo.write("in/kept.txt", b"in\n")?;
    repo.git(&["add", "-A"])?;
    repo.git(&["commit", "-q", "-m", "in"])?;
    repo.git(&["sparse-checkout", "set", "--cone", "in"])?;
    std::fs::create_dir_all(repo.path().join("out")).map_err(|e| e.to_string())?;
    repo.write("out/new.txt", b"out\n")?;
    repo.write("in/kept.txt", b"changed\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&["ls-tree", "-r", "--name-only", "stash@{0}^3"])?;
    assert_eq!(stored.trim(), "out/new.txt");
    Ok(())
}
