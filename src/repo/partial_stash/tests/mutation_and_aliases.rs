#[test]
fn stash_job_waits_in_the_shared_worktree_mutation_lane() -> Result<(), String> {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let repo = Repo::new("mutation-lane")?;
    let peer = tempfile::tempdir().map_err(|e| e.to_string())?;
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "lane-peer",
        peer.path().to_str().ok_or("peer path")?,
    ])?;
    let ctx = egui::Context::default();
    let blocker = RepoTab::new_deferred(repo.path().to_owned(), &ctx);
    let mut tab = RepoTab::new_deferred(peer.path().to_owned(), &ctx);
    assert_eq!(
        crate::tasks::mutation_schedule_key(blocker.tx.repo.clone()),
        crate::tasks::mutation_schedule_key(tab.tx.repo.clone())
    );
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel::<()>();
    crate::repo::messages::spawn_mutation(
        &blocker.tx,
        &ctx,
        |error| done("lane blocker", Err(error), OpKind::plain()),
        move || {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            done("lane blocker", Ok(String::new()), OpKind::plain())
        },
    );
    started_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    let (ran_tx, ran_rx) = channel();
    tab.stash_job("recorded stash lane", &ctx, move |_| {
        let _ = ran_tx.send(());
        (Ok("recorded stash finished".into()), OpKind::plain())
    });
    assert!(
        crate::tasks::pool()?.queued_priority_for(&tab.tx.cancel)
            == Some(crate::tasks::Priority::Background)
    );
    assert!(ran_rx.try_recv().is_err());
    drop(release_tx);
    ran_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    let msg = tab
        .rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|e| e.to_string())?;
    assert!(
        matches!(&msg, Msg::OpDone { label, result: Ok(_), .. } if label == "recorded stash lane")
    );
    tab.handle(msg, &ctx);
    assert!(tab.busy_op.is_none());
    Ok(())
}

#[test]
fn filesystem_alias_guard_uses_repository_entries_and_preserves_distinct_case() -> Result<(), String>
{
    let repo = Repo::new("filesystem-alias-guard")?;
    // Two names for one owned file model filesystem aliases without mounts or a
    // destructive stash. -ef also handles true case aliases on the target filesystem.
    repo.write("A", b"excluded bytes\n")?;
    std::fs::hard_link(repo.path().join("A"), repo.path().join("alias"))
        .map_err(|e| e.to_string())?;
    let work = WorkStatus {
        staged: vec![git::StatusFile {
            path: "A".into(),
            old_path: None,
            code: 'D',
            area: git::Area::Staged,
        }],
        unstaged: vec![git::StatusFile {
            path: "alias".into(),
            old_path: None,
            code: '?',
            area: git::Area::Untracked,
        }],
        ..Default::default()
    };
    assert!(check_filesystem_overlaps(repo.path(), &work, false, &[]).is_err());
    assert!(check_filesystem_overlaps(repo.path(), &work, true, &[]).is_ok());
    assert!(check_filesystem_overlaps(repo.path(), &work, true, &["alias"]).is_err());
    std::fs::create_dir(repo.path().join("directory")).map_err(|e| e.to_string())?;
    let mut nested = work.clone();
    nested.staged[0].path = "directory/removed".into();
    assert!(check_filesystem_overlaps(repo.path(), &nested, false, &["directory"]).is_err());
    assert!(check_filesystem_overlaps(repo.path(), &work, false, &["unrelated"]).is_err()); // ordinary alias still excluded
    let mut distinct = work.clone();
    distinct.unstaged.clear();
    repo.write("a", b"other bytes\n")?;
    let case_alias = same_file::is_same_file(repo.path().join("A"), repo.path().join("a"))
        .map_err(|e| e.to_string())?;
    assert_eq!(
        check_filesystem_overlaps(repo.path(), &distinct, false, &["a"]).is_err(),
        case_alias
    );
    // Paths remain original quoted data, including shell metacharacters.
    distinct.staged[0].path = "quote'$(never-run)".into();
    let script = filesystem_overlap_script(&distinct, true, &["quote'$(never-run)/child"])?;
    assert!(script.contains("'\\''"));
    assert!(script.contains("./quote"));
    assert!(check_filesystem_overlaps(repo.path(), &distinct, true, &["unrelated"]).is_ok());
    Ok(())
}

#[cfg(unix)]
#[test]
fn unresolved_excluded_symlinks_refuse_normal_push_and_redo() -> Result<(), String> {
    for (ignored, untracked) in [(false, false), (true, false), (true, true)] {
        let repo = Repo::new(&format!("dangling-alias-{ignored}-{untracked}"))?;
        repo.write("A", b"tracked contents\n")?;
        repo.git(&["add", "A"])?;
        repo.git(&["commit", "-qm", "tracked alias candidate"])?;
        repo.write("a.txt", b"eligible queued change\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, untracked);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        repo.git(&["rm", "A"])?;
        std::os::unix::fs::symlink("missing-target", repo.path().join("a"))
            .map_err(|e| e.to_string())?;
        if ignored {
            std::fs::write(repo.path().join(".git/info/exclude"), "/a\n")
                .map_err(|e| e.to_string())?;
        }
        // On a case-insensitive target A and a can be one dangling directory entry.
        // Refuse unresolved links conservatively even on this case-sensitive fixture.
        let before = repo.state()?;
        let head = git::head(repo.path());
        let stashes = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|e| e.contains("unresolved symbolic link")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo,
            OpKind::Redo(undone.clone()),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|e| e.contains("unresolved symbolic link")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(git::head(repo.path()), head);
        assert_eq!(repo.git(&["stash", "list"])?, stashes);
        assert_eq!(
            std::fs::read_link(repo.path().join("a")).map_err(|e| e.to_string())?,
            PathBuf::from("missing-target")
        );
        assert!(everything_problem(&repo.status()?, untracked).is_none());
    }
    Ok(())
}

#[test]
fn unstaged_file_replaced_by_ignored_directory_refuses_push_and_redo() -> Result<(), String> {
    for untracked in [false, true] {
        let repo = Repo::new(if untracked {
            "unstaged-ignored-with-u"
        } else {
            "unstaged-ignored-without-u"
        })?;
        repo.write(".gitignore", b"a.txt/local\n")?;
        repo.git(&["add", ".gitignore"])?;
        repo.git(&["commit", "-q", "-m", "ignore replacement"])?;
        repo.write("a.txt", b"changed\n")?;
        let request = StashRequest {
            mode: StashMode::Everything,
            message: String::new(),
            untracked,
        };
        let stale_args = push_args(&request, &repo.status()?)?;
        let (result, kind) = repo.stash(StashMode::Everything, untracked);
        result?;
        let undone = repo.undo(entry(kind)?)?;
        std::fs::remove_file(repo.path().join("a.txt")).map_err(|e| e.to_string())?;
        std::fs::create_dir(repo.path().join("a.txt")).map_err(|e| e.to_string())?;
        repo.write("a.txt/local", b"excluded replacement bytes\n")?;
        let work = repo.status()?;
        assert!(work.staged.is_empty());
        assert!(
            work.unstaged
                .iter()
                .any(|f| f.path == "a.txt" && f.code == 'D')
        );
        assert!(!work.unstaged.iter().any(|f| f.path == "a.txt/local"));
        let before = repo.state()?;
        let head = git::head(repo.path());
        let stash_list = repo.git(&["stash", "list"])?;
        let (result, _) = run_push(repo.path(), stale_args, head.clone(), false);
        assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
        let (result, _, stopped) = run_steps_counted(
            repo.path(),
            &undone.redo,
            OpKind::Redo(undone.clone()),
            None,
            false,
            None,
        );
        assert!(result.is_err_and(|e| e.contains("overlaps a deletion")));
        assert_eq!(stopped, 0);
        assert_eq!(repo.state()?, before);
        assert_eq!(git::head(repo.path()), head);
        assert_eq!(repo.git(&["stash", "list"])?, stash_list);
        assert!(everything_problem(&work, untracked).is_none());
        let (result, _) = run_snapshot(repo.path(), "safe snapshot", untracked, head);
        result?;
        assert_eq!(repo.state()?, before);
    }
    Ok(())
}
