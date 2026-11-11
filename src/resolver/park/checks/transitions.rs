use super::*;

pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("gitcraft-real-effects-{}-{n}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join(".git")).unwrap();
        std::fs::write(path.join("file"), b"\xffraw\0\r\n").unwrap();
        std::fs::write(path.join("ignored"), b"\xffignored").unwrap();
        std::fs::write(path.join(".git/index"), b"DIRC-original").unwrap();
        std::fs::write(path.join(".git/MERGE_HEAD"), b"theirs\r\n").unwrap();
        crate::git::reset_refs(&path);
        trace(None);
        Self(path)
    }
    pub(super) fn park(&self) -> Parked {
        park(&self.0, "test-id", &["file".into()]).unwrap()
    }
    pub(super) fn proof(&self) {
        assert_eq!(
            std::fs::read(self.0.join("ignored")).unwrap(),
            b"\xffignored"
        );
        let original = |base: &Path, raw: &str| {
            std::fs::read(base.join(raw)).ok().as_deref() == Some(b"\xffraw\0\r\n")
                && std::fs::read(base.join("index")).ok().as_deref() == Some(b"DIRC-original")
                && std::fs::read(base.join("MERGE_HEAD")).ok().as_deref() == Some(b"theirs\r\n")
        };
        let live = std::fs::read(self.0.join("file")).unwrap() == b"\xffraw\0\r\n"
            && original(&self.0.join(".git"), "../file");
        let archive = discovery::archives(&self.0)
            .unwrap()
            .iter()
            .any(|p| original(p, "raw/file"));
        assert!(
            live || archive,
            "neither exact live bytes nor discoverable archive"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        trace(None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn trace(stop: Option<usize>) {
    effects::TRACE.with(|t| *t.borrow_mut() = (Vec::new(), stop));
}
pub(super) fn labels() -> Vec<String> {
    effects::TRACE.with(|t| t.borrow().0.clone())
}

#[test]
fn actual_park_restore_drop_boundaries_keep_bytes_and_restart_discovery() {
    for action in ["park", "restore", "drop"] {
        let fixture = Fixture::new();
        let parked = if action == "park" {
            None
        } else {
            Some(fixture.park())
        };
        trace(None);
        match action {
            "park" => {
                fixture.park();
            }
            "restore" => bring_back(&fixture.0, parked.as_ref().unwrap()).unwrap(),
            _ => {
                drop_to_branch(&fixture.0, parked.as_ref().unwrap()).unwrap();
            }
        }
        let boundaries = labels();
        if action == "park" {
            let reachable = boundaries
                .iter()
                .position(|s| s == "archive-reachable")
                .unwrap();
            let clear = boundaries
                .iter()
                .position(|s| s.starts_with("git:reset"))
                .unwrap();
            assert!(reachable < clear);
        }
        if action == "restore" {
            assert!(boundaries.iter().any(|s| s == "live-restore-synced"));
        }
        for stop in 1..=boundaries.len() {
            let fixture = Fixture::new();
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
            fixture.proof();
            let first = list(&fixture.0).unwrap();
            let second = list(&fixture.0).unwrap();
            assert_eq!(first, second, "{action} boundary {stop}");
            fixture.proof();
            if let Some(parked) = parked {
                if let Err(e) = bring_back(&fixture.0, &parked) {
                    assert!(
                        e.contains("Interrupted recovery")
                            || e.contains("progress")
                            || e.contains("locked")
                            || e.contains("Commit or stash"),
                        "{e}"
                    );
                }
                fixture.proof();
                drop_to_branch(&fixture.0, &parked).unwrap();
                let branch = drop_to_branch(&fixture.0, &parked).unwrap();
                assert!(branch.starts_with("parked/"));
                assert_eq!(drop_to_branch(&fixture.0, &parked).unwrap(), branch);
                assert_eq!(list(&fixture.0).unwrap().len(), 1);
                super::recording_regressions::refs_clean(&fixture, &parked);
                fixture.proof();
            }
        }
        eprintln!(
            "{action}: {} actual effect interruption points",
            boundaries.len()
        );
    }
}

#[test]
fn copy_restart_resumes_and_new_work_is_never_rolled_back() {
    let fixture = Fixture::new();
    let parked = fixture.park();
    // Derive stop relative to bring_back only.
    trace(None);
    bring_back(&fixture.0, &parked).unwrap();
    let stops: Vec<usize> = labels()
        .iter()
        .enumerate()
        .filter_map(|(n, s)| (s == "restore-copy-applied").then_some(n + 1))
        .collect();
    for stop in stops {
        for edit in [false, true] {
            let fixture = Fixture::new();
            let parked = fixture.park();
            trace(Some(stop));
            assert!(bring_back(&fixture.0, &parked).is_err());
            trace(None);
            if edit {
                std::fs::write(fixture.0.join("file"), b"new-user-work").unwrap();
            }
            let result = bring_back(&fixture.0, &parked);
            if edit {
                assert!(result.is_err());
                assert_eq!(
                    std::fs::read(fixture.0.join("file")).unwrap(),
                    b"new-user-work"
                );
            } else {
                result.unwrap();
            }
            fixture.proof();
        }
    }
}

#[test]
fn receipts_reconcile_only_known_legacy_tail_and_ignore_temporary_files() {
    let fixture = Fixture::new();
    let parked = fixture.park();
    let dir = folder(&fixture.0, &parked.id).unwrap();
    std::fs::write(dir.join("journal-v1/99999999.pending"), b"torn-temp").unwrap();
    assert_eq!(journal::latest(&dir).unwrap(), Some(journal::Phase::Parked));
    let tail = dir.join("journal-v1/99999999.json");
    std::fs::write(&tail, br#"{"version":1,"phase":"Rest"#).unwrap();
    assert_eq!(
        journal::latest(&dir).unwrap(),
        Some(journal::Phase::Uncertain)
    );
    assert!(tail.with_extension("torn").exists());
    journal::record(&dir, journal::Phase::Parked).unwrap();
    std::fs::write(&tail, br#"{"version":99,"phase":"Restored"}"#).unwrap();
    assert!(journal::latest(&dir).is_err());
    fixture.proof();
}

#[test]
fn restored_receipt_does_not_retire_changed_live_files() {
    let fixture = Fixture::new();
    let parked = fixture.park();
    let dir = folder(&fixture.0, &parked.id).unwrap();
    journal::record(&dir, journal::Phase::Restored).unwrap();
    std::fs::write(fixture.0.join("file"), b"new-work").unwrap();
    assert_eq!(list(&fixture.0).unwrap(), vec![parked.clone()]);
    assert!(dir.exists());
    assert!(bring_back(&fixture.0, &parked).is_err());
    assert_eq!(std::fs::read(fixture.0.join("file")).unwrap(), b"new-work");
    fixture.proof();
}

#[test]
fn source_identity_and_foreign_lock_refuse_writes() {
    let fixture = Fixture::new();
    let parked = fixture.park();
    let dir = folder(&fixture.0, &parked.id).unwrap();
    std::fs::write(fixture.0.join(".git/index.lock"), b"foreign-owner").unwrap();
    assert!(bring_back(&fixture.0, &parked).is_err());
    assert_eq!(
        std::fs::read(fixture.0.join(".git/index.lock")).unwrap(),
        b"foreign-owner"
    );
    assert_eq!(std::fs::read(fixture.0.join("file")).unwrap(), b"clean");
    let mut other = parked.clone();
    other.snapshot = "wrong-source".into();
    assert!(bring_back(&fixture.0, &other).is_err());
    assert!(drop_to_branch(&fixture.0, &other).is_err());
    assert_eq!(journal::latest(&dir).unwrap(), Some(journal::Phase::Parked));
    fixture.proof();
}
