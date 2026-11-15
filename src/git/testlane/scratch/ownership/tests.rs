use super::*;
use crate::git::testlane::{Scratch, fixture::Repo};

#[test]
fn recovery_requires_matching_records_and_dead_owner_evidence() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let dir = allocation::reserve(root.path(), "test")?;
    assert!(Ownership::verified(&dir).is_none());
    let owner = Ownership::create(&dir)?;
    assert!(owner.matches(&dir));
    assert!(!owner.abandoned(&owner.machine, owner.boot.as_deref(), |_| true));
    assert!(owner.abandoned(&owner.machine, owner.boot.as_deref(), |_| false));
    assert!(!owner.abandoned("another-machine", None, |_| false));
    let mut boot_owner = owner.clone();
    boot_owner.boot = Some("previous-boot".into());
    assert!(
        boot_owner.abandoned(&owner.machine, Some("current-boot"), |_| {
            assert!(false, "a changed boot must not probe a reused PID");
            true
        })
    );
    let mut forged = owner.clone();
    forged.token = "0".repeat(32);
    remote::write(
        &remote::join(&dir, MARKER),
        &serde_json::to_vec(&forged).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(Ownership::verified(&dir).is_none());
    assert!(owner.finish(&dir).is_err());
    assert!(dir.exists());
    Ok(())
}

fn abandoned(root: &Path, n: u64) -> Result<(PathBuf, Ownership), String> {
    let name = ScratchName {
        label: "test".into(),
        machine: this_machine(),
        pid: 4_000_000_000,
        n,
    };
    let dir = root.join(name.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut owner = Ownership::create(&dir)?;
    owner.pid = name.pid;
    let bytes = serde_json::to_vec(&owner).map_err(|e| e.to_string())?;
    for path in [
        remote::join(&dir, MARKER),
        registry(&dir).ok_or("registry")?,
    ] {
        remote::write(&path, &bytes).map_err(|e| e.to_string())?;
    }
    Ok((dir, owner))
}
#[test]
fn crash_recovery_removes_owned_clean_worktrees_but_preserves_dirty_or_unproven_ones()
-> Result<(), String> {
    let repo = Repo::new("scratch-owned-recovery")?;
    repo.commit("base", &[("tracked", Some("original"))])?;
    let root = super::super::scratch_root(repo.path())?;
    let (clean, _) = abandoned(&root, 100)?;
    let (dirty, _) = abandoned(&root, 101)?;
    let (mismatched, _) = abandoned(&root, 102)?;
    for dir in [&clean, &dirty, &mismatched] {
        repo.git(&[
            "worktree",
            "add",
            "--detach",
            &dir.join("checkout").to_string_lossy(),
            "HEAD",
        ])?;
    }
    std::fs::write(dirty.join("checkout/tracked"), b"uncommitted").map_err(|e| e.to_string())?;
    std::fs::write(mismatched.join(MARKER), b"{}").map_err(|e| e.to_string())?;
    let refs = repo.git(&["show-ref"])?;
    let next = Scratch::add_empty(repo.path(), "test", "HEAD")?;
    assert!(!clean.exists());
    assert!(!registry(&clean).ok_or("registry")?.exists());
    assert!(dirty.join("checkout.preserved").exists());
    assert_eq!(
        std::fs::read(dirty.join("checkout/tracked")).map_err(|e| e.to_string())?,
        b"uncommitted"
    );
    assert!(mismatched.join("checkout/tracked").exists());
    assert_eq!(refs, repo.git(&["show-ref"])?);
    next.remove()?;
    // Only the fixture explicitly removes its retained worktrees, after assertions.
    for dir in [&dirty, &mismatched] {
        repo.git(&[
            "worktree",
            "remove",
            "--force",
            &dir.join("checkout").to_string_lossy(),
        ])?;
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::remove_file(registry(dir).ok_or("registry")?).map_err(|e| e.to_string())?;
    }
    Ok(())
}
