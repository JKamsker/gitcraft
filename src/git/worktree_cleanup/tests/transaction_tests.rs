//! Deterministic late writers, recovery/restore, and explicit purge boundaries.
use std::io::Write;

use crate::git::worktree_cleanup::archive::Store;
use crate::git::worktree_cleanup::tests::{Fixture, write};
use crate::git::worktree_cleanup::{Outcome, inspect, remove};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn ignored_write_after_final_inspection_is_never_deleted() -> Result {
    let f = Fixture::new("late ignored capture")?;
    let wt = f.merged_worktree("done")?;
    f.repo.git(&[
        "config",
        "core.excludesFile",
        &f.scratch.join("ignore").to_string_lossy(),
    ])?;
    write(&f.scratch, "ignore", b".env\n")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = super::remove_after_inspection(
        f.repo.path(),
        &target,
        &preview,
        Some(&f.store),
        false,
        || assert!(std::fs::write(wt.join(".env"), "late secret").is_ok()),
    );
    assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
    assert_eq!(std::fs::read_to_string(wt.join(".env"))?, "late secret");
    Ok(())
}

#[test]
fn late_descriptor_writes_survive_archive_verification_and_cleanup() -> Result {
    let f = Fixture::new("descriptor survives capture")?;
    let wt = f.merged_worktree("done")?;
    write(&wt, "notes.txt", b"preview\n")?;
    let mut writer = std::fs::OpenOptions::new()
        .append(true)
        .open(wt.join("notes.txt"))?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    let outcome = remove(f.repo.path(), &target, &preview, Some(&f.store), true);
    // Windows may refuse to rename an open file's parent; that refusal must preserve it.
    if cfg!(windows) && matches!(outcome, Outcome::Failed(_)) {
        assert!(wt.join("notes.txt").exists());
        return Ok(());
    }
    let Outcome::Removed {
        recovery,
        archive: Some(archive),
        freed,
    } = outcome
    else {
        return Err(format!("not retained: {outcome:?}").into());
    };
    assert_eq!(std::path::Path::new(&archive.manifest.worktree), wt);
    writer.write_all(b"after verification\n")?;
    writer.sync_all()?;
    assert_eq!(
        std::fs::read_to_string(recovery.join("notes.txt"))?,
        "preview\nafter verification\n"
    );
    assert_eq!(freed.bytes, 0);
    assert!(!wt.exists());
    let items = Store::recoveries(f.repo.path())?;
    assert_eq!(items.len(), 1);
    assert!(items[0].0.join("git-state/index").is_file());
    // Restore refuses collisions, preserving both copies.
    std::fs::create_dir(&wt)?;
    write(&wt, "user.txt", b"do not overwrite")?;
    assert!(Store::restore_recovery(f.repo.path(), &items[0].0).is_err());
    assert_eq!(std::fs::read(wt.join("user.txt"))?, b"do not overwrite");
    std::fs::remove_file(wt.join("user.txt"))?;
    std::fs::remove_dir(&wt)?;
    drop(writer);
    Store::restore_recovery(f.repo.path(), &items[0].0)?;
    assert!(wt.join("notes.txt").is_file());
    assert!(
        crate::git::worktrees(f.repo.path(), None)?
            .iter()
            .any(|w| w.path == wt)
    );
    Ok(())
}

#[test]
fn purge_is_explicit_and_rejects_other_repositories() -> Result {
    let f = Fixture::new("purge retained")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Removed { .. }
    ));
    let items = Store::recoveries(f.repo.path())?;
    let directory = &items[0].0;
    let other = Fixture::new("purge wrong repository")?;
    assert!(Store::purge_recovery(other.repo.path(), directory).is_err());
    assert!(directory.join("tree").exists());
    #[cfg(unix)]
    {
        let outside = f.scratch.join("outside");
        std::fs::create_dir(&outside)?;
        write(&outside, "precious", b"keep")?;
        std::os::unix::fs::symlink(&outside, directory.join("tree/link"))?;
        Store::purge_recovery(f.repo.path(), directory)?;
        assert_eq!(std::fs::read(outside.join("precious"))?, b"keep");
    }
    #[cfg(not(unix))]
    Store::purge_recovery(f.repo.path(), directory)?;
    assert!(!directory.exists());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn cross_device_capture_refuses_without_copying_or_removing() -> Result {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new("cross device capture")?;
    let outside = tempfile::tempdir_in("/dev/shm")?;
    if std::fs::metadata(outside.path())?.dev() == std::fs::metadata(f.repo.path())?.dev() {
        return Ok(());
    }
    f.repo.git(&["branch", "done", "main"])?;
    let wt = outside.path().join("worktree");
    f.repo
        .git(&["worktree", "add", &wt.to_string_lossy(), "done"])?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Failed(_)
    ));
    assert!(wt.join("base.txt").exists());
    assert!(Store::recoveries(f.repo.path())?.is_empty());
    Ok(())
}

#[test]
fn missing_retained_tree_cannot_restore_a_phantom_registration() -> Result {
    let f = Fixture::new("missing recovery tree")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Removed { .. }
    ));
    let items = Store::recoveries(f.repo.path())?;
    let directory = &items[0].0;
    std::fs::rename(directory.join("tree"), f.scratch.join("moved-by-user"))?;
    assert!(Store::restore_recovery(f.repo.path(), directory).is_err());
    assert!(directory.join("git-state/index").is_file());
    assert!(!wt.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn symlinked_recovery_area_is_refused_before_moving_content() -> Result {
    let f = Fixture::new("recovery root symlink")?;
    let wt = f.merged_worktree("done")?;
    let own = f.repo.path().join(".git/gitcraft");
    std::fs::create_dir_all(&own)?;
    std::os::unix::fs::symlink(&f.scratch, own.join("recovery"))?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Failed(_)
    ));
    assert!(wt.join("done.txt").exists());
    Ok(())
}
