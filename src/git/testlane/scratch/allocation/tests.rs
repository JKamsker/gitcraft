use super::*;
use crate::git::testlane::{Scratch, fixture::Repo};
#[test]
fn exclusive_claim_never_adopts_or_removes_existing_content() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = root.path().join("retained");
    assert!(claim(&path)?);
    std::fs::write(path.join("uncommitted"), b"keep").map_err(|e| e.to_string())?;
    assert!(!claim(&path)?);
    discard_empty(&path);
    assert_eq!(
        std::fs::read(path.join("uncommitted")).map_err(|e| e.to_string())?,
        b"keep"
    );
    Ok(())
}
#[test]
fn failed_add_cannot_delete_a_retained_worktree_from_a_reused_pid() -> Result<(), String> {
    let repo = Repo::new("scratch-retained-collision")?;
    repo.commit("base", &[])?;
    let root = super::super::scratch_root(repo.path())?;
    let name = ScratchName {
        label: "test".into(),
        machine: this_machine(),
        pid: std::process::id(),
        n: 0,
    };
    let retained = root.join(name.to_string());
    repo.git(&[
        "worktree",
        "add",
        "--detach",
        "--no-checkout",
        &retained.to_string_lossy(),
        "HEAD",
    ])?;
    std::fs::write(retained.join("uncommitted"), b"precious").map_err(|e| e.to_string())?;
    let marker = retained.with_extension("preserved");
    std::fs::write(&marker, b"preserved").map_err(|e| e.to_string())?;
    assert!(!claim(&retained)?);
    let before = repo.git(&["worktree", "list", "--porcelain"])?;
    assert!(Scratch::add_empty(repo.path(), "test", "refs/heads/missing").is_err());
    assert_eq!(before, repo.git(&["worktree", "list", "--porcelain"])?);
    let next = Scratch::add_empty(repo.path(), "test", "HEAD")?;
    assert_ne!(next.path, retained);
    next.remove()?;
    assert_eq!(
        std::fs::read(retained.join("uncommitted")).map_err(|e| e.to_string())?,
        b"precious"
    );
    assert!(marker.exists());
    repo.git(&["worktree", "remove", "--force", &retained.to_string_lossy()])?;
    std::fs::remove_file(marker).map_err(|e| e.to_string())?;
    Ok(())
}
#[test]
fn reservations_are_distinct_and_failed_empty_attempts_are_cleaned() -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let first = reserve(root.path(), root.path(), "test")?;
    let second = reserve(root.path(), root.path(), "test")?;
    assert_ne!(first, second);
    discard_empty(&first);
    discard_empty(&second);
    assert!(!first.exists() && !second.exists());
    Ok(())
}
