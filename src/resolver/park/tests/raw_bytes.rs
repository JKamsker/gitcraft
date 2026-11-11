#[test]
fn a_changed_file_comes_back_byte_for_byte_whatever_the_filters() -> Result<(), String> {
    let repo = mid_merge("park-bytes")?;
    let attributes = repo.path().join(".git/info/attributes");
    std::fs::write(&attributes, "notes.txt text eol=crlf\n").map_err(|e| e.to_string())?;
    let parked = park(repo.path(), "b1", &["a.txt".to_owned()])?;
    bring_back(repo.path(), &parked)?;
    let notes = std::fs::read(repo.path().join("notes.txt")).map_err(|e| e.to_string())?;
    assert_eq!(notes, b"notes, changed meanwhile\n", "not with CRLF");
    Ok(())
}

#[test]
fn a_file_the_merge_deleted_on_disk_again_keeps_it_from_being_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-recreated")?;
    // Out of the merge's index, the file on disk the user's own: the reset would write the
    // branch's over it.
    repo.git(&["rm", "-q", "--cached", "notes.txt"])?;
    let before = state(&repo)?;
    let refused = park(repo.path(), "r1", &["a.txt".to_owned()]);
    assert!(
        refused.as_ref().is_err_and(|e| e.contains("notes.txt")),
        "{refused:?}"
    );
    assert_eq!(state(&repo)?, before, "nothing changed");
    assert!(list(repo.path())?.is_empty(), "nothing kept");
    Ok(())
}

#[test]
fn a_merge_put_aside_already_isnt_written_over() -> Result<(), String> {
    let repo = mid_merge("park-taken")?;
    let taken = crate::agent::repo_file(repo.path(), "parked/t1")?;
    std::fs::create_dir_all(&taken).map_err(|e| e.to_string())?;
    std::fs::write(taken.join("index"), "kept before").map_err(|e| e.to_string())?;
    let before = state(&repo)?;
    assert!(park(repo.path(), "t1", &["a.txt".to_owned()]).is_err());
    assert_eq!(read(&taken, "index").as_deref(), Some("kept before"));
    assert_eq!(state(&repo)?, before, "nothing changed");
    Ok(())
}

#[test]
fn a_failure_once_it_is_coming_back_leaves_it_clean_and_put_aside() -> Result<(), String> {
    let repo = mid_merge("park-half")?;
    let before = state(&repo)?;
    let parked = park(repo.path(), "k1", &["a.txt".to_owned()])?;
    let dir = folder(repo.path(), "k1")?;
    let (raw, away) = (dir.join("raw/a.txt"), dir.join("a.txt.away"));
    std::fs::rename(&raw, &away).map_err(|e| e.to_string())?;
    // Missing required bytes must fail preflight before checkout, receipts or a copy plan.
    assert!(bring_back(repo.path(), &parked).is_err());
    assert!(!dir.join("restore-plan.json").exists());
    assert_eq!(journal::latest(&dir)?, Some(journal::Phase::Parked));
    assert!(!merging_now(&repo));
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?,
        ""
    );
    assert_eq!(read(repo.path(), "new.txt"), None);
    assert_eq!(list(repo.path())?.len(), 1, "still put aside");
    std::fs::rename(&away, &raw).map_err(|e| e.to_string())?;
    bring_back(repo.path(), &parked)?;
    assert_eq!(state(&repo)?, before);
    Ok(())
}

#[test]
fn restoration_rejects_both_hidden_index_flags() -> Result<(), String> {
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let repo = mid_merge(&format!("park-hidden-{flag}"))?;
        let parked = park(repo.path(), "hidden", &["a.txt".into()])?;
        repo.git(&["update-index", flag, "notes.txt"])?;
        write(repo.path(), "notes.txt", "hidden user edit\n")?;
        assert!(
            repo.git(&["status", "--porcelain", "--untracked-files=no"])?
                .is_empty()
        );
        #[cfg(unix)]
        let inherited = locking::tests::Inherited::new();
        let error = bring_back(repo.path(), &parked).unwrap_err();
        assert!(!error.contains("is busy"), "{error}");
        #[cfg(unix)]
        assert_eq!(inherited.count(), 1);
        assert_eq!(
            read(repo.path(), "notes.txt").as_deref(),
            Some("hidden user edit\n")
        );
        assert_eq!(list(repo.path())?, vec![parked]);
    }
    Ok(())
}

#[test]
fn freshness_rejects_raw_changes_that_git_normalizes_away() -> Result<(), String> {
    let repo = mid_merge("park-raw-freshness")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    let gd = git_dir(repo.path())?;
    let dir = folder(repo.path(), "raw-freshness")?;
    reserve(&dir)?;
    let (_, theirs) = operation(repo.path(), &gd)?;
    let parked = keep(
        repo.path(),
        &gd,
        &dir,
        "raw-freshness",
        "merge",
        &theirs,
        &["a.txt".into()],
    )?;
    assert!(unchanged(repo.path(), &gd, &dir, &parked.snapshot));
    write(repo.path(), "notes.txt", "notes, changed meanwhile\r\n")?;
    // Same filtered tree, different user bytes: the pre-reset guard must stop.
    let tree = trees(repo.path(), &gd.join("index"))?.1;
    assert_eq!(
        tree,
        trimmed(run(
            repo.path(),
            &["rev-parse", &format!("{}^{{tree}}", parked.snapshot)]
        )?)
    );
    assert!(!unchanged(repo.path(), &gd, &dir, &parked.snapshot));
    Ok(())
}

#[test]
fn dropping_to_a_branch_keeps_accessible_byte_exact_recovery() -> Result<(), String> {
    let repo = mid_merge("park-raw-recovery")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    let raw = b"notes, changed meanwhile\r\n";
    std::fs::write(repo.path().join("notes.txt"), raw).map_err(|e| e.to_string())?;
    let parked = park(repo.path(), "raw-recovery", &["a.txt".into()])?;
    let branch = drop_to_branch(repo.path(), &parked)?;
    let archived = crate::agent::repo_file(repo.path(), "parked-recovery/raw-recovery/parked")?;
    assert_eq!(
        std::fs::read(archived.join("raw/notes.txt")).map_err(|e| e.to_string())?,
        raw
    );
    assert!(archived.join("index").is_file());
    assert!(archived.join("parked.json").is_file());
    let description = repo.git(&["config", &format!("branch.{branch}.description")])?;
    let recorded = description
        .strip_prefix("Original byte-exact files and index: ")
        .ok_or_else(|| format!("missing recovery path in branch description: {description:?}"))?
        .trim_end_matches(['\r', '\n']);
    // Windows accepts both separators, even when Path::join and embedded '/'
    // produce different display strings. The advertised path must reach our archive.
    assert_eq!(
        std::fs::canonicalize(recorded).map_err(|e| e.to_string())?,
        std::fs::canonicalize(&archived).map_err(|e| e.to_string())?,
        "branch description points to the wrong recovery archive: {description:?}"
    );
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    assert!(
        repo.git(&["show", &format!("{branch}:notes.txt")])?
            .ends_with("\n")
    );
    Ok(())
}

#[test]
fn a_filtered_clean_file_is_also_kept_byte_exact() -> Result<(), String> {
    let repo = mid_merge("park-clean-raw")?;
    write(repo.path(), ".gitattributes", "notes.txt text eol=lf\n")?;
    repo.git(&["add", ".gitattributes"])?;
    write(repo.path(), "notes.txt", "notes\r\n")?;
    assert!(repo.git(&["diff", "--", "notes.txt"])?.is_empty());
    let parked = park(repo.path(), "clean-raw", &["a.txt".into()])?;
    assert!(parked.raw.contains(&"notes.txt".into()));
    bring_back(repo.path(), &parked)?;
    assert_eq!(
        std::fs::read(repo.path().join("notes.txt")).map_err(|e| e.to_string())?,
        b"notes\r\n"
    );
    Ok(())
}

#[test]
fn recovery_archive_collision_keeps_both_originals() -> Result<(), String> {
    let repo = mid_merge("park-recovery-collision")?;
    let parked = park(repo.path(), "collision", &["a.txt".into()])?;
    let archive = crate::agent::repo_file(repo.path(), "parked-recovery/collision")?;
    std::fs::create_dir_all(&archive).map_err(|e| e.to_string())?;
    std::fs::write(archive.join("existing"), b"mine").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let inherited = locking::tests::Inherited::new();
    let error = drop_to_branch(repo.path(), &parked).unwrap_err();
    assert!(!error.contains("is busy"), "{error}");
    #[cfg(unix)]
    assert_eq!(inherited.count(), 1);
    assert_eq!(
        std::fs::read(archive.join("existing")).map_err(|e| e.to_string())?,
        b"mine"
    );
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    assert!(folder(repo.path(), &parked.id)?.join("raw/a.txt").is_file());
    Ok(())
}

#[test]
fn filtered_clean_edit_after_parking_is_refused_before_any_writes() -> Result<(), String> {
    let repo = mid_merge("park-post-clean-raw")?;
    // Use persistent attributes on HEAD, not only attributes stored in the parked merge.
    let attributes = git_dir(repo.path())?.join("info/attributes");
    std::fs::write(attributes, "notes.txt text eol=lf\n").map_err(|e| e.to_string())?;
    let parked = park(repo.path(), "post-clean-raw", &["a.txt".into()])?;
    write(repo.path(), "notes.txt", "notes\r\n")?;
    // Refresh Git's stat cache through its clean filter: the indexed blob stays HEAD's.
    repo.git(&["add", "notes.txt"])?;
    assert!(
        repo.git(&["status", "--porcelain", "--untracked-files=no"])?
            .is_empty()
    );
    let before = state(&repo)?;
    let index = std::fs::read(git_dir(repo.path())?.join("index")).map_err(|e| e.to_string())?;
    let error = bring_back(repo.path(), &parked)
        .err()
        .ok_or("must retain hidden user edit")?;
    assert!(error.contains("raw bytes changed"), "{error}");
    assert_eq!(state(&repo)?, before);
    assert_eq!(
        std::fs::read(git_dir(repo.path())?.join("index")).map_err(|e| e.to_string())?,
        index
    );
    assert_eq!(list(repo.path())?, vec![parked.clone()]);
    assert!(
        folder(repo.path(), &parked.id)?
            .join("raw/notes.txt")
            .is_file()
    );
    write(repo.path(), "notes.txt", "notes\n")?;
    repo.git(&["add", "notes.txt"])?;
    bring_back(repo.path(), &parked)?;
    Ok(())
}
