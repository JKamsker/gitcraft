// Recovery. Included to preserve libtest shard names.
/// Cancelling stops every test running and removes the worktrees.
#[test]
fn cancelling_stops_the_tests_and_removes_the_worktrees() -> Result<(), String> {
    let dir = scratch("cancel")?;
    let repo = Repo::new(&dir.join("repo"), 20, &|_| vec!["slow"])?;
    let started = Instant::now();
    let mut child_started = false;
    let (outcome, _) = bisect_with(
        repo.setup(&command(&dir.join("trees")), 4),
        &TreeCache::default(),
        &mut |event, handle| {
            if matches!(event, Event::Round { .. }) {
                // Round is emitted before workspace setup and child dispatch. Wait
                // for this fixture's actual start marker, even under suite load.
                let deadline = Instant::now() + Duration::from_secs(30);
                while Instant::now() < deadline {
                    if std::fs::read_to_string(dir.join("trees.pids"))
                        .is_ok_and(|text| text.lines().any(|line| !line.trim().is_empty()))
                    {
                        child_started = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                // Cancel even if startup failed, so the fixture always joins its work.
                handle.cancel();
            }
        },
    );
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(
        child_started,
        "no owned slow child reached its start marker"
    );
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
    #[cfg(unix)]
    assert!(
        sleepers_stopped(&dir.join("trees"))?,
        "a cancelled test still runs"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Completed durable owners and a released claim let the next claimant recover worktrees
/// and generation refs; a still-held claim prevents takeover regardless of PID or time.
#[test]
fn a_crashed_bisect_is_cleared_up_by_the_next() -> Result<(), String> {
    let dir = scratch("leftovers")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 4)?;
    let leave_behind = || -> Result<workspace::Workspace, String> {
        let ws = workspace::Workspace::open(&repo.dir, &Machine::Here, None)?;
        let (_, slot) = ws.slot(1);
        let limit = Duration::from_secs(60);
        let added = ws.run_git(
            &repo.dir,
            &["worktree", "add", "-q", "--detach", &slot, &repo.commits[2]],
            limit,
        )?;
        assert_eq!(added.code, Some(0), "{:?}", added.stderr);
        let authority = ws.authority(crate::remote::workspace_authority::FenceMode::Mutation)?;
        let reference = format!("refs/gitcraft-bisect/{}/sent-0", authority.generation);
        let published = ws.run_git(
            &repo.dir,
            &["update-ref", &reference, &repo.commits[3]],
            limit,
        )?;
        assert_eq!(published.code, Some(0), "{:?}", published.stderr);
        let mut records = 0;
        for entry in std::fs::read_dir(&authority.root).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|ext| ext == "owner") {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                assert_eq!(value["v"], 3);
                assert_eq!(value["generation"], authority.generation);
                assert_eq!(crate::remote::recover_owner(&path)?, bytes);
                records += 1;
            }
        }
        assert!(records >= 2, "real operations must publish durable owners");
        Ok(ws)
    };
    let ws = leave_behind()?;
    let base = ws.base.clone();
    let (outcome, _) = bisect(repo.setup(&command(&dir.join("t1")), 2));
    assert!(
        matches!(&outcome, Outcome::Failed(e) if e.contains("another workspace claimant")),
        "{outcome:?}"
    );
    assert!(base.join("bisect-1").exists());
    ws.leave_completed_fixture()?;
    workspace::clear_leftovers(&repo.dir);
    repo.assert_clean()?;
    let ws = leave_behind()?;
    ws.leave_completed_fixture()?;
    let (outcome, events) = bisect(repo.setup(&command(&dir.join("t2")), 2));
    assert_eq!(
        outcome,
        Outcome::FirstBad(repo.commits[4].clone()),
        "{events:?}"
    );
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A legacy timestamp/PID file cannot authorize cleanup, even if it looks ancient.
#[test]
fn legacy_crash_evidence_is_refused_and_retained() -> Result<(), String> {
    let dir = scratch("legacy-leftovers")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 10, 4)?;
    let (base, _) = workspace::base_for(&repo.dir, &Machine::Here)?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let slot = sh_path(&base.join("bisect-1"));
    git::run(
        Some(&repo.dir),
        &["worktree", "add", "-q", "--detach", &slot, &repo.commits[2]],
    )?;
    let reference = "refs/gitcraft-bisect/sent-0";
    git::run(
        Some(&repo.dir),
        &["update-ref", reference, &repo.commits[3]],
    )?;
    let evidence = b"gitcraft 1 1000\n";
    std::fs::write(base.join("owner"), evidence).map_err(|e| e.to_string())?;
    let (outcome, _) = bisect(repo.setup(&command(&dir.join("trees")), 2));
    assert!(matches!(outcome, Outcome::Failed(e) if e.contains("unresolved legacy evidence")));
    workspace::clear_leftovers(&repo.dir);
    assert_eq!(
        std::fs::read(base.join("owner")).map_err(|e| e.to_string())?,
        evidence
    );
    assert!(base.join("bisect-1").exists());
    assert_eq!(
        git::run(Some(&repo.dir), &["rev-parse", reference])?.trim(),
        repo.commits[3]
    );
    // Only this fixture's completed Git operations created these artifacts; no owners
    // or processes are inferred from the deliberately unauthenticated legacy text.
    git::run(Some(&repo.dir), &["worktree", "remove", "--force", &slot])?;
    git::run(
        Some(&repo.dir),
        &["update-ref", "-d", reference, &repo.commits[3]],
    )?;
    std::fs::remove_file(base.join("owner")).map_err(|e| e.to_string())?;
    std::fs::remove_dir(&base).map_err(|e| e.to_string())?;
    repo.assert_clean()?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// Clearing up never forgets the user's own worktrees, not even one whose folder is away (an
/// unmounted drive): only the bisect's.
#[test]
fn clearing_up_keeps_the_user_s_worktrees() -> Result<(), String> {
    let dir = scratch("own-worktrees")?;
    let repo = Repo::with_culprit(&dir.join("repo"), 8, 4)?;
    let away = dir.join("away");
    git::run(
        Some(&repo.dir),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &sh_path(&away),
            &repo.commits[1],
        ],
    )?;
    std::fs::rename(&away, dir.join("moved")).map_err(|e| e.to_string())?;
    let (outcome, _) = bisect(repo.setup("test ! -e bug", 3));
    assert_eq!(outcome, Outcome::FirstBad(repo.commits[4].clone()));
    let list = git::run(Some(&repo.dir), &["worktree", "list", "--porcelain"])?;
    assert!(list.contains("away"), "{list}");
    assert_eq!(list.matches("worktree ").count(), 2, "{list}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
