use super::*;
use crate::git::file_comparison::{LensSpec, read_lens, tests::Repo};
use std::os::unix::fs::PermissionsExt;
fn record<T, E: std::fmt::Display>(result: Result<T, E>, error: &mut Option<String>) -> bool {
    match result {
        Ok(_) => true,
        Err(e) => {
            *error = Some(e.to_string());
            false
        }
    }
}
fn fixture() -> Result<(Repo, Lens, String), String> {
    let r = Repo::new()?;
    r.write("a", "before\n")?;
    r.write("unrelated", "unchanged\n")?;
    r.commit()?;
    r.write("a", "after\n")?;
    let lens = r.lens("a")?;
    let patch = String::from(
        "diff --git \"a/a\" \"b/a\"\n--- \"a/a\"\n+++ \"b/a\"\n@@ -1 +1 @@\n-before\n+after\n",
    );
    Ok((r, lens, patch))
}
fn bytes(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| e.to_string())
}
#[test]
fn foreign_lock_regular_symlink_and_last_moment_replacements_are_preserved() -> Result<(), String> {
    for (phase, symlink) in [
        (Phase::Prepared, false),
        (Phase::Prepared, true),
        (Phase::BeforeInstall, false),
        (Phase::DuringUnlock, false),
    ] {
        let (r, lens, patch) = fixture()?;
        let idx = r.0.join(".git/index");
        let lock = r.0.join(".git/index.lock");
        let original = bytes(&idx)?;
        let outside = r.0.join("sentinel");
        std::fs::write(&outside, b"outside-original").map_err(|e| e.to_string())?;
        let displaced = r.0.join("owned-displaced-lock");
        let mut injected = false;
        let mut operation_hook_error: Option<String> = None;
        let result = apply(&r.0, &lens, &patch, false, &mut |at| {
            if at == phase {
                if !record(
                    std::fs::rename(&lock, &displaced),
                    &mut operation_hook_error,
                ) {
                    return;
                }
                if symlink {
                    if !record(
                        std::os::unix::fs::symlink(&outside, &lock),
                        &mut operation_hook_error,
                    ) {
                        return;
                    }
                } else {
                    if !record(
                        std::fs::write(&lock, b"FOREIGN-LOCK-BYTES"),
                        &mut operation_hook_error,
                    ) {
                        return;
                    }
                }
                injected = true;
            }
        });
        if let Some(e) = operation_hook_error {
            return Err(e);
        }
        assert!(injected && result.is_err(), "{phase:?}: {result:?}");
        assert_eq!(bytes(&outside)?, b"outside-original");
        assert!(std::fs::symlink_metadata(&lock).is_ok());
        if !symlink {
            assert_eq!(bytes(&lock)?, b"FOREIGN-LOCK-BYTES");
        }
        assert_ne!(bytes(&idx)?, b"FOREIGN-LOCK-BYTES");
        if phase == Phase::Prepared {
            assert_eq!(bytes(&idx)?, original);
        }
        assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
        assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
    }
    Ok(())
}
#[test]
fn swapped_index_parent_and_last_moment_index_creator_are_recoverable() -> Result<(), String> {
    for parent_swap in [true, false] {
        let (r, lens, patch) = fixture()?;
        let gitdir = r.0.join(".git");
        let saved = r.0.join("saved-git");
        let old = bytes(&gitdir.join("index"))?;
        let mut injected = false;
        let mut operation_hook_error: Option<String> = None;
        let result = apply(&r.0, &lens, &patch, false, &mut |at| {
            if parent_swap && at == Phase::BeforeMove {
                if !record(std::fs::rename(&gitdir, &saved), &mut operation_hook_error) {
                    return;
                }
                if !record(std::fs::create_dir(&gitdir), &mut operation_hook_error) {
                    return;
                }
                if !record(
                    std::fs::write(gitdir.join("index"), b"FOREIGN-PARENT"),
                    &mut operation_hook_error,
                ) {
                    return;
                }
                injected = true;
            } else if !parent_swap && at == Phase::BeforeInstall {
                if !record(
                    std::fs::write(gitdir.join("index"), b"LATE-CREATOR"),
                    &mut operation_hook_error,
                ) {
                    return;
                }
                injected = true;
            }
        });
        if let Some(e) = operation_hook_error {
            return Err(e);
        }
        assert!(injected && result.is_err());
        if parent_swap {
            assert_eq!(bytes(&gitdir.join("index"))?, b"FOREIGN-PARENT");
            assert_eq!(bytes(&saved.join("index"))?, old);
        } else {
            assert_eq!(bytes(&gitdir.join("index"))?, b"LATE-CREATOR");
            assert!(
                std::fs::read_dir(&gitdir)
                    .map_err(|e| e.to_string())?
                    .filter_map(Result::ok)
                    .any(|e| bytes(&e.path().join("previous")).is_ok_and(|b| b == old))
            );
        }
    }
    Ok(())
}
#[test]
fn split_index_keeps_unrelated_entries_and_reverses_through_the_new_publisher() -> Result<(), String>
{
    let (r, _, patch) = fixture()?;
    r.git(&["update-index", "--split-index"])?;
    let lens = r.lens("a")?;
    apply(&r.0, &lens, &patch, false, &mut |_| {})?;
    assert_eq!(r.git(&["show", ":a"])?, "after");
    assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
    apply(&r.0, &r.lens("a")?, &patch, true, &mut |_| {})?;
    assert_eq!(r.git(&["show", ":a"])?, "before");
    assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
    Ok(())
}
#[test]
fn index_owner_comparison_and_real_group_drift_refuse_stale_publication() -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let (r, lens, patch) = fixture()?;
    let parent = Directory::open(&r.0.join(".git")).map_err(|e| e.to_string())?;
    let before = Snapshot::read(&parent, OsStr::new("index"))?.ok_or("index")?;
    let mut synthetic = Snapshot::read(&parent, OsStr::new("index"))?.ok_or("index")?;
    synthetic.uid = synthetic.uid.wrapping_add(1);
    assert!(!synthetic.same(&before));
    let idx = r.0.join(".git/index");
    let group = std::fs::metadata(&idx).map_err(|e| e.to_string())?.gid();
    let groups = rustix::process::getgroups().map_err(|e| e.to_string())?;
    if let Some(gid) = groups.iter().find(|g| g.as_raw() != group) {
        let mut hook_error = None;
        let result = apply(&r.0, &lens, &patch, false, &mut |phase| {
            if phase == Phase::Prepared
                && let Err(e) = std::os::unix::fs::chown(&idx, None, Some(gid.as_raw()))
            {
                hook_error = Some(e.to_string());
            }
        });
        if let Some(e) = hook_error {
            return Err(e);
        }
        assert!(result.is_err());
        assert_eq!(bytes(&idx)?, before.bytes);
    }
    Ok(())
}

mod policy;
