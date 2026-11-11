// These reservation tests use only owned filesystem entries: no Git, process,
// platform metadata helper, credential flow or transport is invoked.
#[cfg(unix)]
#[test]
fn lock_release_preserves_regular_and_symlink_replacements_on_all_returns() -> Result<(), String> {
    for is_symlink in [false, true] {
        for operation_ok in [false, true] {
            let temp = crate::git::file_comparison::Scratch::new()?;
            let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
            let mut lock = Lock::new(parent, OsStr::new("merged"))?;
            let public = temp.0.join(lock.name());
            let displaced = temp.0.join("owned-moved-aside");
            let outside = temp.0.join("outside");
            write(&outside, b"OUTSIDE")?;
            let recovery = lock.recovery_path()?;
            let mut hook_error = None;
            let operation = if operation_ok {
                Ok(())
            } else {
                Err("original preparation failure".into())
            };
            let result = lock.finish(operation, &mut |at| {
                if at != Phase::BeforeUnlock {
                    return;
                }
                let action = std::fs::rename(&public, &displaced).and_then(|_| {
                    if is_symlink {
                        std::os::unix::fs::symlink(&outside, &public)
                    } else {
                        std::fs::write(&public, b"FOREIGN-LOCK-BYTES")
                    }
                });
                if let Err(e) = action {
                    hook_error = Some(e.to_string());
                }
            });
            if let Some(error) = hook_error {
                return Err(error);
            }
            let error = result
                .err()
                .ok_or("Foreign replacement reported clean success")?;
            assert!(error.contains("Save lock release uncertain"));
            assert!(error.contains(&recovery.to_string_lossy().to_string()));
            assert_eq!(
                error.contains("original preparation failure"),
                !operation_ok
            );
            drop(lock); // No second attempt is allowed to remove the restored owner.
            if is_symlink {
                assert_eq!(
                    std::fs::read_link(&public).map_err(|e| e.to_string())?,
                    outside
                );
                assert_eq!(
                    std::fs::read_link(recovery.join("released-lock")).map_err(|e| e.to_string())?,
                    outside
                );
            } else {
                assert_eq!(read(&public)?, b"FOREIGN-LOCK-BYTES");
                assert_eq!(
                    read(&recovery.join("released-lock"))?,
                    b"FOREIGN-LOCK-BYTES"
                );
            }
            assert_eq!(read(&outside)?, b"OUTSIDE");
            assert_eq!(read(&displaced)?, b"");
        }
    }
    Ok(())
}
#[test]
fn lock_release_retains_captured_foreign_entry_when_new_owner_occupies_public_name()
-> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
    let mut lock = Lock::new(parent, OsStr::new("merged"))?;
    let public = temp.0.join(lock.name());
    let recovery = lock.recovery_path()?;
    let mut error = None;
    let result = lock.finish(Ok(()), &mut |at| {
        let action = match at {
            Phase::BeforeUnlock => std::fs::rename(&public, temp.0.join("owned"))
                .and_then(|_| std::fs::write(&public, b"CAPTURED-FOREIGN")),
            Phase::AfterLockCapture => std::fs::write(&public, b"NEW-OWNER"),
            _ => Ok(()),
        };
        if let Err(e) = action {
            error = Some(e.to_string());
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    assert!(result.is_err());
    drop(lock);
    assert_eq!(read(&public)?, b"NEW-OWNER");
    assert_eq!(read(&recovery.join("released-lock"))?, b"CAPTURED-FOREIGN");
    Ok(())
}
#[cfg(unix)]
#[test]
fn lock_release_uncertainty_reaches_actual_save_success_and_error_returns() -> Result<(), String> {
    for operation_ok in [false, true] {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let merged = temp.0.join("merged");
        let output = Output::capture(&merged, &[])?;
        let public = temp.0.join(format!(
            ".gitcraft-merge-{:x}.lock",
            md5::compute(b"merged")
        ));
        let mut fixture_error = None;
        let result = output.save_with(b"candidate", &[], &mut |at| {
            let action = if at == Phase::BeforeInstall && !operation_ok {
                std::fs::write(&merged, b"CONCURRENT-OUTPUT")
            } else if at == Phase::BeforeUnlock {
                std::fs::rename(&public, temp.0.join("owned"))
                    .and_then(|_| std::fs::write(&public, b"FOREIGN"))
            } else {
                Ok(())
            };
            if let Err(e) = action {
                fixture_error = Some(e.to_string());
            }
        });
        if let Some(error) = fixture_error {
            return Err(error);
        }
        let error = result.err().ok_or("Save caller missed lock uncertainty")?;
        assert!(error.contains("Save lock release uncertain"));
        assert_eq!(
            error.contains("Could not install without overwrite"),
            !operation_ok
        );
        assert_eq!(read(&public)?, b"FOREIGN");
        assert_eq!(
            read(&merged)?,
            if operation_ok {
                b"candidate".as_slice()
            } else {
                b"CONCURRENT-OUTPUT".as_slice()
            }
        );
    }
    Ok(())
}
#[test]
fn lock_release_reports_missing_reservation_and_preserves_original_error() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
    let mut lock = Lock::new(parent, OsStr::new("merged"))?;
    std::fs::rename(temp.0.join(lock.name()), temp.0.join("moved")).map_err(|e| e.to_string())?;
    let error = lock
        .finish::<()>(Err("preparation refused".into()), &mut |_| {})
        .err()
        .ok_or("Missing reservation was ignored")?;
    assert!(error.contains("preparation refused") && error.contains("release uncertain"));
    Ok(())
}
#[test]
fn lock_release_normal_and_drop_paths_remove_only_owned_reservations() -> Result<(), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
    for explicit in [false, true] {
        let mut lock = Lock::new(Arc::clone(&parent), OsStr::new("merged"))?;
        let public = temp.0.join(lock.name());
        let recovery = lock.recovery_path()?;
        if explicit {
            lock.finish(Ok(()), &mut |_| {})?;
        }
        drop(lock);
        assert!(!public.exists() && !recovery.exists());
    }
    let lock = Lock::new(parent, OsStr::new("merged"))?;
    let public = temp.0.join(lock.name());
    std::fs::rename(&public, temp.0.join("owned"))
        .and_then(|_| std::fs::write(&public, b"FOREIGN"))
        .map_err(|e| e.to_string())?;
    drop(lock);
    assert_eq!(read(&public)?, b"FOREIGN");
    Ok(())
}
