//! Only owned temporary directories are changed; no real mounts or process signals.
use crate::git::worktree_cleanup::archive::Store;
use crate::git::worktree_cleanup::tests::Fixture;
use crate::git::worktree_cleanup::{Outcome, inspect, remove};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn retained(f: &Fixture) -> std::result::Result<std::path::PathBuf, String> {
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert!(matches!(
        remove(f.repo.path(), &target, &preview, Some(&f.store), false),
        Outcome::Removed { .. }
    ));
    Store::recoveries(f.repo.path())?
        .into_iter()
        .next()
        .map(|(path, _)| path)
        .ok_or("Missing recovery".into())
}

#[cfg(unix)]
#[test]
fn ancestor_replaced_after_validation_cannot_redirect_purge() -> Result {
    let f = Fixture::new("purge ancestor replacement")?;
    let entry = retained(&f)?;
    let root = entry.parent().ok_or("Missing parent")?;
    let parked = root.with_extension("parked");
    let name = entry.file_name().ok_or("Missing name")?;
    let outcome = super::purge_after_validation(
        f.repo.path(),
        &entry,
        || {
            assert!(std::fs::rename(root, &parked).is_ok());
            assert!(std::fs::create_dir(root).is_ok());
            assert!(std::fs::create_dir(root.join(name)).is_ok());
            assert!(std::fs::write(root.join(name).join("precious"), "outside recovery").is_ok());
        },
        |_| Ok(None),
    );
    assert!(outcome.is_err());
    assert_eq!(
        std::fs::read_to_string(entry.join("precious"))?,
        "outside recovery"
    );
    assert!(parked.join(name).join("tree/base.txt").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn entry_replaced_after_validation_preserves_both_trees() -> Result {
    let f = Fixture::new("purge entry replacement")?;
    let entry = retained(&f)?;
    let parked = entry.with_extension("parked");
    assert!(
        super::purge_after_validation(
            f.repo.path(),
            &entry,
            || {
                assert!(std::fs::rename(&entry, &parked).is_ok());
                assert!(std::fs::create_dir(&entry).is_ok());
                assert!(std::fs::write(entry.join("precious"), "replacement").is_ok());
            },
            |_| Ok(None)
        )
        .is_err()
    );
    assert!(parked.join("tree/base.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(entry.join("precious"))?,
        "replacement"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn held_ancestors_prevent_windows_replacement() -> Result {
    let f = Fixture::new("purge windows ancestry")?;
    let entry = retained(&f)?;
    let root = entry.parent().ok_or("Missing parent")?;
    super::purge_after_validation(
        f.repo.path(),
        &entry,
        || {
            assert!(std::fs::rename(root, root.with_extension("parked")).is_err());
            assert!(std::fs::rename(&entry, entry.with_extension("parked")).is_err());
        },
        |_| Ok(None),
    )?;
    assert!(!entry.exists());
    Ok(())
}

#[cfg(unix)]
thread_local! {
    static DURING_TRAVERSAL: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(unix)]
pub(super) fn traversal_hook() {
    let callback = DURING_TRAVERSAL.with(|hook| hook.borrow_mut().take());
    if let Some(callback) = callback {
        callback();
    }
}

#[cfg(unix)]
#[test]
fn restore_and_second_purge_are_excluded_while_a_child_fd_is_being_traversed() -> Result {
    let f = Fixture::new("purge restore exclusion")?;
    let entry = retained(&f)?;
    let repo = f.repo.path().to_owned();
    let target = entry.clone();
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = called.clone();
    DURING_TRAVERSAL.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            assert!(Store::restore_recovery(&repo, &target).is_err());
            assert!(Store::purge_recovery(&repo, &target, |_| Ok(None)).is_err());
            assert!(target.join("tree/base.txt").is_file());
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
        }))
    });
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            DURING_TRAVERSAL.with(|hook| hook.borrow_mut().take());
        }
    }
    let _reset = Reset;
    Store::purge_recovery(f.repo.path(), &entry, |_| Ok(None))?;
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!entry.exists());
    assert!(!f.scratch.join("tree done").exists());
    Ok(())
}

#[test]
fn separate_lock_handles_contend_and_unlock_on_drop() -> Result {
    let f = Fixture::new("recovery lock lifetime")?;
    let entry = retained(&f)?;
    let root = entry.parent().ok_or("Missing root")?;
    let held = super::lock::acquire(root, &entry)?;
    assert!(super::lock::acquire(root, &entry).is_err());
    assert!(Store::restore_recovery(f.repo.path(), &entry).is_err());
    drop(held);
    let next = super::lock::acquire(root, &entry)?;
    drop(next);
    Store::restore_recovery(f.repo.path(), &entry)?;
    Ok(())
}

#[test]
fn capture_keeps_restore_and_purge_excluded_until_rollback_finishes() -> Result {
    let f = Fixture::new("capture recovery exclusion")?;
    let wt = f.merged_worktree("done")?;
    let target = f.target(&wt, "done")?;
    let capture = super::Capture::move_tree(f.repo.path(), &target)?;
    assert!(Store::restore_recovery(f.repo.path(), &capture.directory).is_err());
    assert!(Store::purge_recovery(f.repo.path(), &capture.directory, |_| Ok(None)).is_err());
    capture.rollback("injected failure");
    assert!(wt.join("base.txt").is_file());
    Ok(())
}
