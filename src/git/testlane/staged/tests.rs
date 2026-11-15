use super::*;
use crate::git::testlane::fixture::Repo;

#[test]
fn forgotten_helper_and_partial_stage_test_exact_index_without_source_changes() -> Result<(), String>
{
    let repo = Repo::new("staged-helper")?;
    repo.commit("base", &[("caller.sh", Some("exit 0\n"))])?;
    std::fs::write(repo.path().join("caller.sh"), "test -f helper\n").map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("helper"), "new helper").map_err(|e| e.to_string())?;
    repo.git(&["add", "caller.sh"])?;
    std::fs::write(
        repo.path().join("caller.sh"),
        "test -f helper # unstaged edit\n",
    )
    .map_err(|e| e.to_string())?;
    let index = std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    let refs = repo.git(&["show-ref"])?;
    let status = repo.git(&["status", "--porcelain=v1"])?;
    assert_eq!(
        run_command(repo.path(), "sh caller.sh", &git::Stopper::default())?
            .0
            .code,
        Some(0)
    );
    let failed = test_staged(repo.path(), "sh caller.sh", &RunStop::default())?;
    assert_eq!(failed.ended.code, Some(1));
    assert_eq!(
        index,
        std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?
    );
    assert_eq!(refs, repo.git(&["show-ref"])?);
    assert_eq!(status, repo.git(&["status", "--porcelain=v1"])?);
    assert_eq!(repo.worktree_count()?, 1);
    repo.git(&["add", "helper"])?;
    assert_ne!(
        failed.key.fingerprint,
        staged_fingerprint(repo.path(), "sh caller.sh")?
    );
    assert_eq!(
        test_staged(repo.path(), "sh caller.sh", &RunStop::default())?
            .ended
            .code,
        Some(0)
    );
    Ok(())
}

#[test]
fn additions_deletions_renames_modes_and_ignored_assets() -> Result<(), String> {
    let repo = Repo::new("staged-tree")?;
    repo.commit(
        "base",
        &[
            ("old", Some("old")),
            ("gone", Some("gone")),
            (".gitignore", Some("asset\n")),
        ],
    )?;
    repo.git(&["mv", "old", "renamed"])?;
    repo.git(&["rm", "gone"])?;
    repo.git(&["update-index", "--chmod=+x", "renamed"])?;
    std::fs::write(repo.path().join("asset"), "ignored").map_err(|e| e.to_string())?;
    let (scratch, tree) = snapshot(repo.path(), None)?;
    assert_eq!(
        git::run(Some(&scratch.path), &["rev-parse", "HEAD^{tree}"])?.trim(),
        tree
    );
    assert!(git::run(Some(&scratch.path), &["status", "--porcelain"])?.is_empty());
    assert_eq!(
        std::fs::read(scratch.path.join("renamed")).map_err(|e| e.to_string())?,
        b"old"
    );
    for name in ["old", "gone", "asset"] {
        assert!(!scratch.path.join(name).exists());
    }
    assert!(repo.git(&["ls-tree", &tree])?.contains("100755 blob"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            std::fs::metadata(scratch.path.join("renamed"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
    scratch.remove()?;
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn unresolved_entries_and_submodules_fail_and_clean_up() -> Result<(), String> {
    let repo = Repo::new("staged-unresolved")?;
    let head = repo.commit("base", &[("file", Some("base"))])?;
    repo.git(&["checkout", "-b", "other"])?;
    repo.commit("other", &[("file", Some("other"))])?;
    repo.git(&["checkout", "main"])?;
    repo.commit("main", &[("file", Some("main"))])?;
    assert!(repo.git(&["merge", "other"]).is_err());
    assert!(snapshot(repo.path(), None).is_err());
    assert_eq!(repo.worktree_count()?, 1);
    repo.git(&["merge", "--abort"])?;
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{head},sub"),
    ])?;
    assert!(snapshot(repo.path(), None).is_err());
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn unborn_and_split_indexes_are_supported() -> Result<(), String> {
    let repo = Repo::new("staged-unborn")?;
    std::fs::write(repo.path().join("new"), "staged").map_err(|e| e.to_string())?;
    repo.git(&["add", "new"])?;
    repo.git(&["update-index", "--split-index"])?;
    let (scratch, _) = snapshot(repo.path(), None)?;
    assert!(scratch.path.join("new").exists());
    scratch.remove()?;
    assert!(repo.rev("HEAD").is_err());
    Ok(())
}

#[test]
fn pre_cancelled_run_creates_no_snapshot() -> Result<(), String> {
    let repo = Repo::new("staged-pre-cancel")?;
    let stop = RunStop::default();
    stop.stop();
    assert!(test_staged(repo.path(), "exit 0", &stop).is_err());
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn capture_rejects_a_replaced_index_even_if_it_changes_back_later() -> Result<(), String> {
    let repo = Repo::new("staged-race")?;
    repo.commit("base", &[("file", Some("base"))])?;
    let original = std::fs::read(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    std::fs::write(repo.path().join("file"), "new").map_err(|e| e.to_string())?;
    repo.git(&["add", "file"])?;
    assert!(snapshot(repo.path(), Some(&original)).is_err());
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}

#[test]
fn cancellation_of_owned_snapshot_command_cleans_checkout() -> Result<(), String> {
    let repo = Repo::new("staged-live-cancel")?;
    repo.commit("base", &[])?;
    let marker = repo
        .path()
        .join("ready")
        .to_string_lossy()
        .replace('\\', "/");
    let marker = marker.strip_prefix("//?/").unwrap_or(&marker);
    let command = format!("echo ready > '{marker}'; while :; do sleep 1; done");
    let stop = RunStop::default();
    let worker_stop = stop.clone();
    let path = repo.path().to_owned();
    let worker = std::thread::spawn(move || test_staged(&path, &command, &worker_stop));
    let until = Instant::now() + Duration::from_secs(30);
    while !repo.path().join("ready").exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = repo.path().join("ready").exists();
    stop.stop();
    let result = worker.join().map_err(|_| "snapshot worker panicked")??;
    assert!(ready, "command never reached its owned snapshot");
    assert!(result.ended.stopped);
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

#[test]
fn synthetic_cancellation_tokens_cover_preparation_without_affecting_other_runs() {
    let parent = crate::tasks::Cancellation::default();
    let unrelated = git::Stopper::default();
    crate::tasks::with_cancellation(parent.clone(), || {
        let run = RunStop::default();
        let preparation = run.command_stopper();
        preparation.with_scope(|| {
            let process = git::Stopper::scoped();
            run.stop();
            assert!(preparation.stopped());
            assert!(process.stopped());
        });
        let tab_run = RunStop::default();
        let preparation = tab_run.command_stopper();
        parent.cancel();
        assert!(preparation.stopped());
    });
    assert!(!unrelated.stopped());
}

#[test]
fn cancelled_tab_scope_still_removes_its_snapshot() -> Result<(), String> {
    let repo = Repo::new("staged-tab-cleanup")?;
    repo.commit("base", &[])?;
    let parent = crate::tasks::Cancellation::default();
    crate::tasks::with_cancellation(parent.clone(), || -> Result<(), String> {
        let (scratch, _) = snapshot(repo.path(), None)?;
        parent.cancel();
        drop(scratch);
        Ok(())
    })?;
    assert_eq!(repo.worktree_count()?, 1);
    assert_eq!(repo.scratch_left(), 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn materialization_never_runs_source_checkout_hooks() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new("staged-hooks")?;
    repo.commit("base", &[])?;
    let marker = repo.path().join("hook-ran");
    let hook = repo.path().join(".git/hooks/post-checkout");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\necho unsafe > '{}'\n", marker.display()),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let (scratch, _) = snapshot(repo.path(), None)?;
    scratch.remove()?;
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn intent_to_add_and_staged_empty_file_have_different_freshness() -> Result<(), String> {
    let repo = Repo::new("staged-intent")?;
    repo.commit("base", &[])?;
    std::fs::write(repo.path().join("empty"), "").map_err(|e| e.to_string())?;
    repo.git(&["add", "-N", "empty"])?;
    let before = staged_fingerprint(repo.path(), "exit 0")?;
    let listing = repo.git(&["ls-files", "--stage"])?;
    repo.git(&["add", "empty"])?;
    assert_eq!(listing, repo.git(&["ls-files", "--stage"])?);
    assert_ne!(before, staged_fingerprint(repo.path(), "exit 0")?);
    Ok(())
}
