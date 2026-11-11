#[test]
fn files_git_is_told_not_to_look_at_keep_the_merge_from_being_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-assume")?;
    repo.git(&["update-index", "--assume-unchanged", "notes.txt"])?;
    let before = state(&repo)?;
    let refused = park(repo.path(), "a1", &["a.txt".to_owned()]);
    assert!(refused.is_err_and(|e| e.contains("assume-unchanged")));
    assert_eq!(state(&repo)?, before, "nothing changed");
    assert!(list(repo.path())?.is_empty(), "nothing kept");
    assert!(
        repo.git(&["rev-parse", "-q", "--verify", "refs/gitcraft/parked/a1"])
            .is_err()
    );
    Ok(())
}

#[test]
fn a_locked_index_stops_it_and_it_stays_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-locked")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "l1", &["a.txt".to_owned()])?;
    let lock = repo.path().join(".git/index.lock");
    std::fs::write(&lock, "").map_err(|e| e.to_string())?;
    let refused = bring_back(repo.path(), &parked);
    // What stopped it is said, not files in the way that aren't.
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("index.lock") && !e.contains("doesn't track")),
        "{refused:?}"
    );
    std::fs::remove_file(&lock).map_err(|e| e.to_string())?;
    assert_eq!(list(repo.path())?.len(), 1);
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}
