use super::*;
use crate::git::testlane::fixture::Repo;
#[test]
fn absent_index_capture_is_checked_even_when_the_source_returns_to_absent() -> Result<(), String> {
    let repo = Repo::new("staged-absent-aba")?;
    assert!(index_bytes(repo.path())?.is_none());
    let fingerprint = staged_fingerprint(repo.path(), "exit 0")?;
    std::fs::write(repo.path().join("helper"), b"new").map_err(|e| e.to_string())?;
    repo.git(&["add", "helper"])?;
    let result = snapshot(repo.path(), Some(None));
    std::fs::remove_file(repo.path().join(".git/index")).map_err(|e| e.to_string())?;
    assert_eq!(fingerprint, staged_fingerprint(repo.path(), "exit 0")?);
    assert_eq!(result.err(), Some(crate::i18n::text("staged-stale").into()));
    assert_eq!(repo.worktree_count()?, 1);
    Ok(())
}
#[test]
fn command_failure_remains_explicit_and_cannot_look_like_a_pass() {
    let (ended, error) = command_end(Err("command timed out".into()));
    assert_eq!(ended.code, None);
    assert!(ended.stopped);
    assert_eq!(error.as_deref(), Some("command timed out"));
    let (ended, error) = command_end(Ok(Ended {
        code: Some(0),
        stopped: false,
    }));
    assert_eq!(ended.code, Some(0));
    assert!(error.is_none());
}
