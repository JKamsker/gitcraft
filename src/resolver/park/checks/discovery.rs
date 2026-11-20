//! Real listing/retirement interleavings, with recording Git and owned temporary files.
use super::recording_checks::Fixture;
use super::recording_regressions::refs_clean;
use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn listing_snapshot_survives_foreground_retirement_across_callers() {
    for restore in [false, true] {
        let f = Fixture::new();
        let p = f.park();
        let repo = f.0.clone();
        let (paused, pause) = mpsc::sync_channel(0);
        let (resume, resumed) = mpsc::sync_channel(0);
        let listing = std::thread::spawn(move || {
            effects::HOOK.with(|h| {
                *h.borrow_mut() = Some(Box::new(move |label| {
                    if label == "discovery-snapshot" {
                        paused.send(()).unwrap();
                        resumed.recv_timeout(Duration::from_secs(10)).unwrap();
                    }
                }));
            });
            let found = list(&repo);
            effects::HOOK.with(|h| *h.borrow_mut() = None);
            found
        });
        pause.recv_timeout(Duration::from_secs(10)).unwrap();
        // The listing has enumerated active paths and an empty recovery root.
        // An independent caller now completes the actual foreground operation.
        if restore {
            bring_back(&f.0, &p).unwrap();
        } else {
            drop_to_branch(&f.0, &p).unwrap();
        }
        assert!(!folder(&f.0, &p.id).unwrap().exists());
        resume.send(()).unwrap();
        assert_eq!(listing.join().unwrap().unwrap(), vec![p.clone()]);
        for _ in 0..4 {
            assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
            refs_clean(&f, &p);
            f.proof();
        }
        // Legacy journal-only paths and two discovery entries must not duplicate a row.
        std::fs::create_dir_all(folder(&f.0, &p.id).unwrap().join("journal-v1")).unwrap();
        assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
        refs_clean(&f, &p);
    }
}

#[test]
fn discovery_reports_corrupt_missing_mismatched_and_duplicate_manifests() {
    let f = Fixture::new();
    let p = f.park();
    let source = folder(&f.0, &p.id).unwrap();
    let path = source.join("parked.json");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    assert!(
        list(&f.0)
            .unwrap_err()
            .contains("Invalid recovery manifest")
    );
    std::fs::remove_file(&path).unwrap();
    assert!(
        list(&f.0)
            .unwrap_err()
            .contains("Missing recovery manifest")
    );
    let mut wrong = p.clone();
    wrong.id = "someone-else".into();
    std::fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
    assert!(list(&f.0).unwrap_err().contains("identity mismatch"));
    std::fs::write(&path, &original).unwrap();
    let retained =
        crate::agent::repo_file(&f.0, &format!("parked-recovery/{}/parked", p.id)).unwrap();
    std::fs::create_dir_all(&retained).unwrap();
    // A pre-move empty recovery reservation cannot shadow the active manifest.
    assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
    std::fs::write(retained.join("parked.json"), &original).unwrap();
    assert!(list(&f.0).unwrap_err().contains("collision"));
    std::fs::remove_dir_all(retained).unwrap();
    assert_eq!(list(&f.0).unwrap(), vec![p]);
    f.proof();
}

/// Gap G2: a manifest torn before the Archived receipt (written in place by an older version)
/// is an incomplete reservation; it must not hide the repository's other archives.
#[test]
fn a_torn_manifest_while_archiving_hides_no_other_archive() {
    let f = Fixture::new();
    let p = f.park();
    let torn = folder(&f.0, "torn-id").unwrap();
    std::fs::create_dir(&torn).unwrap();
    journal::record(&torn, journal::Phase::Archiving).unwrap();
    let bytes = br#"{"id":"torn-id","worktree":"#;
    std::fs::write(torn.join("parked.json"), bytes).unwrap();
    effects::NOTES.with(|n| n.borrow_mut().clear());
    for _ in 0..2 {
        assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
    }
    // Reported, so the user can be told, and removable with forget, alone.
    let found = list_all(&f.0).unwrap();
    assert_eq!(
        found,
        (vec![p.clone()], vec![("torn-id".to_owned(), torn.clone())])
    );
    let notes = effects::NOTES.with(|n| n.borrow().clone());
    assert!(
        notes
            .iter()
            .any(|n| n.contains("Incomplete recovery reservation")),
        "{notes:?}"
    );
    assert_eq!(std::fs::read(torn.join("parked.json")).unwrap(), bytes);
    // Once the worktree may have been cleared, or with no receipts at all (a legacy
    // archive), an unreadable manifest is corruption and fails closed.
    journal::record(&torn, journal::Phase::Archived).unwrap();
    assert!(
        list(&f.0)
            .unwrap_err()
            .contains("Invalid recovery manifest")
    );
    std::fs::remove_dir_all(torn.join("journal-v1")).unwrap();
    assert!(
        list(&f.0)
            .unwrap_err()
            .contains("Invalid recovery manifest")
    );
    // Archiving again: reported again, and forget removes it and nothing else.
    journal::record(&torn, journal::Phase::Archiving).unwrap();
    assert_eq!(list_all(&f.0).unwrap().1.len(), 1);
    forget(&f.0, "torn-id");
    assert!(!torn.exists());
    assert_eq!(list_all(&f.0).unwrap(), (vec![p], Vec::new()));
    f.proof();
}
