#[test]
fn a_snapshot_leaves_every_byte_where_it_was() -> Result<(), String> {
    let repo = Repo::new("snapshot")?;
    repo.stage_in_part()?;
    repo.git(&["mv", "old.txt", "moved.txt"])?;
    let _ = std::fs::remove_file(repo.path().join("del.txt"));
    repo.write("bin.dat", &[1, 0, 1, 0])?;
    repo.write(UNICODE, b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    std::fs::create_dir_all(repo.path().join("new dir/deeper")).map_err(|e| e.to_string())?;
    repo.write("new dir/deeper/x y.txt", b"deep\n")?;
    repo.write(".gitignore", b"*.log\n")?;
    repo.write("ignored.log", b"ignored\n")?;
    let before = repo.state()?;
    let index = std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    let (result, kind) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.state()?, before);
    assert_eq!(
        std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?,
        index,
        "the index file changed"
    );
    // No file of its own left behind in .git.
    let leftovers: Vec<String> = std::fs::read_dir(repo.path().join(".git"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("gitcraft-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    // In the stash list like any other, named as asked, on HEAD.
    let list = git::stashes(repo.path()).map_err(|e| e.to_string())?;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].message, "wip ü");
    assert_eq!(list[0].base, git::head(repo.path()));
    // Untracked files in its third parent, ignored ones not.
    let untracked = repo.git(&[
        "-c",
        "core.quotePath=false",
        "ls-tree",
        "-r",
        "--name-only",
        "stash@{0}^3",
    ])?;
    let mut untracked: Vec<&str> = untracked.lines().collect();
    untracked.sort_unstable();
    assert_eq!(untracked, [".gitignore", "new dir/deeper/x y.txt", NEW]);
    // Popped elsewhere, it is the very state it saved.
    let entry_ = entry(kind)?;
    repo.git(&["stash", "push", "-q", "--include-untracked"])?;
    repo.git(&["stash", "drop", "-q"])?;
    // (The one just pushed dropped: the snapshot is the newest again.)
    repo.git(&["stash", "apply", "-q", "--index"])?;
    assert_eq!(repo.state()?, before);
    // Undone: the stash goes, the files stay; redone: it comes back.
    let undone = repo.undo(entry_)?;
    assert_eq!(repo.stashes(), 0);
    assert_eq!(repo.state()?, before);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_snapshot_without_untracked_files_leaves_them_out() -> Result<(), String> {
    let repo = Repo::new("snapshot-tracked")?;
    repo.write("a.txt", b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, false);
    assert!(result.is_ok(), "{result:?}");
    let parents = repo.git(&["log", "-1", "--format=%P", "stash@{0}"])?;
    assert_eq!(parents.split_whitespace().count(), 2);
    assert_eq!(repo.read(NEW), b"untracked\n");
    Ok(())
}

#[test]
fn a_snapshot_of_only_untracked_files_is_made_on_head() -> Result<(), String> {
    let repo = Repo::new("snapshot-untracked")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, false);
    assert!(result.is_err_and(|e| e.contains("Include untracked")));
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let list = git::stashes(repo.path()).map_err(|e| e.to_string())?;
    assert_eq!(list.len(), 1);
    // Its tree is HEAD's, its index HEAD's: popping it brings back just the new file.
    let head_tree = repo.git(&["rev-parse", "HEAD^{tree}"])?;
    assert_eq!(repo.git(&["rev-parse", "stash@{0}^{tree}"])?, head_tree);
    assert_eq!(repo.git(&["rev-parse", "stash@{0}^2^{tree}"])?, head_tree);
    std::fs::remove_file(repo.path().join(NEW)).map_err(|e| e.to_string())?;
    repo.git(&["stash", "pop", "-q", "--index"])?;
    assert_eq!(repo.read(NEW), b"untracked\n");
    assert!(repo.status()?.staged.is_empty());
    Ok(())
}

#[test]
fn a_snapshot_of_nothing_says_so() -> Result<(), String> {
    let repo = Repo::new("snapshot-nothing")?;
    let (result, kind) = repo.stash(StashMode::Snapshot, true);
    assert_eq!(
        result.err().as_deref(),
        Some("there are no changes to save")
    );
    assert!(matches!(kind, OpKind::Normal(Undo::None, _)));
    assert_eq!(repo.stashes(), 0);
    Ok(())
}

#[test]
fn a_snapshot_needs_a_first_commit() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-snapshot-unborn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let repo = Repo(dir);
    repo.git(&["init", "-q"])?;
    repo.write("a.txt", b"a\n")?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_err_and(|e| e.contains("first commit")));
    Ok(())
}

/// Hundreds of untracked files, more than one command line takes, all go in the snapshot.
#[test]
fn a_snapshot_takes_many_untracked_files() -> Result<(), String> {
    let repo = Repo::new("snapshot-many")?;
    std::fs::create_dir_all(repo.path().join("many")).map_err(|e| e.to_string())?;
    let long = "l".repeat(120);
    for i in 0..900 {
        repo.write(&format!("many/{long} {i}.txt"), format!("{i}\n").as_bytes())?;
    }
    let before = repo.state()?;
    let (result, _) = repo.stash(StashMode::Snapshot, true);
    assert!(result.is_ok(), "{result:?}");
    let stored = repo.git(&["ls-tree", "-r", "--name-only", "stash@{0}^3"])?;
    assert_eq!(stored.lines().count(), 900);
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn everything_stashes_untracked_files_only_when_asked() -> Result<(), String> {
    let repo = Repo::new("everything")?;
    repo.write("a.txt", b"changed\n")?;
    repo.write(NEW, b"untracked\n")?;
    let (result, kind) = repo.stash(StashMode::Everything, false);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.read(NEW), b"untracked\n");
    assert_eq!(repo.read("a.txt"), b"a\n");
    repo.undo(entry(kind)?)?;
    let (result, _) = repo.stash(StashMode::Everything, true);
    assert!(result.is_ok(), "{result:?}");
    assert!(repo.status()?.is_clean());
    Ok(())
}
