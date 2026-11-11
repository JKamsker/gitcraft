//! Windows access-policy model on real production I/O; no Git children.
use super::recording_checks::{Fixture, labels, trace};
use super::*;

struct Policy;
impl Policy {
    fn new() -> Self {
        durability::WINDOWS_POLICY.with(|v| v.set(true));
        Self
    }
}
impl Drop for Policy {
    fn drop(&mut self) {
        durability::WINDOWS_POLICY.with(|v| v.set(false));
    }
}
fn readonly(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions).unwrap();
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("untouched"), b"readonly untouched\xff").unwrap();
    readonly(&fixture.0.join("untouched"));
    crate::git::extra_tracked(&fixture.0, "untouched");
    fixture
}
fn proof(fixture: &Fixture) {
    fixture.proof();
    let path = fixture.0.join("untouched");
    assert_eq!(std::fs::read(&path).unwrap(), b"readonly untouched\xff");
    assert!(std::fs::metadata(path).unwrap().permissions().readonly());
}

#[test]
fn readonly_payload_park_restore_drop_and_each_interruption() {
    let _policy = Policy::new();
    for action in ["park", "restore", "drop"] {
        let first = fixture();
        let parked = if action == "park" {
            None
        } else {
            Some(first.park())
        };
        trace(None);
        match action {
            "park" => {
                first.park();
            }
            "restore" => bring_back(&first.0, parked.as_ref().unwrap()).unwrap(),
            _ => {
                drop_to_branch(&first.0, parked.as_ref().unwrap()).unwrap();
            }
        }
        proof(&first);
        let count = labels().len();
        for stop in 1..=count {
            let fixture = fixture();
            let parked = if action == "park" {
                None
            } else {
                Some(fixture.park())
            };
            trace(Some(stop));
            match action {
                "park" => {
                    let _ = park(&fixture.0, "test-id", &["file".into()]);
                }
                "restore" => {
                    let _ = bring_back(&fixture.0, parked.as_ref().unwrap());
                }
                _ => {
                    let _ = drop_to_branch(&fixture.0, parked.as_ref().unwrap());
                }
            }
            trace(None);
            proof(&fixture);
            let listed = list(&fixture.0).unwrap();
            assert_eq!(listed, list(&fixture.0).unwrap());
            if let Some(parked) = parked {
                let _ = bring_back(&fixture.0, &parked);
                drop_to_branch(&fixture.0, &parked).unwrap();
            }
            proof(&fixture);
        }
        eprintln!("readonly {action}: {count} injected boundaries");
    }
}

#[test]
fn restored_readonly_output_keeps_permissions_and_can_restart() {
    let _policy = Policy::new();
    let fixture = fixture();
    let parked = fixture.park();
    let dir = folder(&fixture.0, &parked.id).unwrap();
    // Model original readonly conflict payload restored over a writable Git output.
    readonly(&dir.join("raw/file"));
    trace(None);
    bring_back(&fixture.0, &parked).unwrap();
    assert!(
        std::fs::metadata(fixture.0.join("file"))
            .unwrap()
            .permissions()
            .readonly()
    );
    proof(&fixture);
    assert_eq!(list(&fixture.0).unwrap().len(), 1);
}

#[test]
fn unproven_readonly_and_changed_identity_fail_closed() {
    let _policy = Policy::new();
    let fixture = fixture();
    let parked = fixture.park();
    let dir = folder(&fixture.0, &parked.id).unwrap();
    let path = dir.join("raw/untouched");
    assert!(durability::proven(&dir, "raw/untouched", &path).unwrap());
    // Same bytes and attributes on a different file do not inherit a flush receipt.
    let other = dir.join("other");
    std::fs::copy(&path, &other).unwrap();
    assert!(!durability::proven(&dir, "raw/untouched", &other).unwrap());
    assert!(journal::sync_file(&other).is_err());
    #[cfg(unix)]
    {
        std::fs::rename(&other, &path).unwrap();
        assert!(!durability::proven(&dir, "raw/untouched", &path).unwrap());
        assert!(journal::sync_tree(&dir).is_err());
        assert!(drop_to_branch(&fixture.0, &parked).is_err());
    }
    #[cfg(windows)]
    {
        // Windows refuses replacement of the readonly destination itself.
        assert!(std::fs::rename(&other, &path).is_err());
        assert!(durability::proven(&dir, "raw/untouched", &path).unwrap());
    }
    proof(&fixture);
}
