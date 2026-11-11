use super::recording_checks::{Fixture, labels, trace};
use super::*;

pub(super) fn refs_clean(f: &Fixture, p: &Parked) {
    assert!(run(&f.0, &["rev-parse", "-q", "--verify", &p.ref_name()]).is_err());
    assert_eq!(
        run(
            &f.0,
            &[
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/gitcraft/parked-recovery/{}", p.id)
            ]
        )
        .unwrap(),
        p.snapshot
    );
    let archive = discovery::recovery_location(&f.0, p).unwrap();
    assert_eq!(
        std::fs::read(archive.join("raw/file")).unwrap(),
        b"\xffraw\0\r\n"
    );
    assert_eq!(
        std::fs::read(archive.join("index")).unwrap(),
        b"DIRC-original"
    );
    assert_eq!(
        std::fs::read(archive.join("MERGE_HEAD")).unwrap(),
        b"theirs\r\n"
    );
}

#[test]
fn independent_callers_cannot_retire_between_foreground_receipts() {
    let f = Fixture::new();
    let p = f.park();
    let repo = f.0.clone();
    let parked = p.clone();
    effects::HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |label| {
            if label != "phase:Restored" && label != "archive-moved" {
                return;
            }
            let moved = label == "archive-moved";
            let repo = repo.clone();
            let parked = parked.clone();
            std::thread::spawn(move || {
                // Actual list reconciliation runs in a separate caller while foreground owns lock.
                assert!(list(&repo).unwrap_err().contains("busy"));
                assert!(bring_back(&repo, &parked).unwrap_err().contains("busy"));
                assert!(drop_to_branch(&repo, &parked).unwrap_err().contains("busy"));
                assert_eq!(moved, !folder(&repo, &parked.id).unwrap().exists());
                assert!(
                    discovery::locate(&repo, &parked.id)
                        .unwrap()
                        .join("parked.json")
                        .is_file()
                );
            })
            .join()
            .unwrap();
        }))
    });
    bring_back(&f.0, &p).unwrap();
    effects::HOOK.with(|h| *h.borrow_mut() = None);
    let old = folder(&f.0, &p.id).unwrap();
    assert!(journal::record(&old, journal::Phase::Cleanup).is_err());
    assert!(!old.exists());
    for _ in 0..4 {
        assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
        refs_clean(&f, &p);
    }
    // Legacy journal-only ghosts never shadow the intact retained archive.
    std::fs::create_dir_all(old.join("journal-v1")).unwrap();
    for _ in 0..3 {
        assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
        refs_clean(&f, &p);
    }
}

#[test]
fn required_sources_fail_before_mutation_then_repair_retries() {
    for name in ["raw/file", "index", "MERGE_HEAD"] {
        let f = Fixture::new();
        let p = f.park();
        let dir = folder(&f.0, &p.id).unwrap();
        let source = dir.join(name);
        let bytes = std::fs::read(&source).unwrap();
        std::fs::remove_file(&source).unwrap();
        assert!(
            bring_back(&f.0, &p)
                .unwrap_err()
                .contains("Required recovery source")
        );
        assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"clean");
        assert_eq!(
            std::fs::read(f.0.join(".git/index")).unwrap(),
            b"clean-index"
        );
        assert!(!f.0.join(".git/MERGE_HEAD").exists());
        assert!(!dir.join("restore-plan.json").exists());
        assert_eq!(journal::latest(&dir).unwrap(), Some(journal::Phase::Parked));
        std::fs::write(source, bytes).unwrap();
        bring_back(&f.0, &p).unwrap();
        refs_clean(&f, &p);
    }
}

#[test]
fn abrupt_effect_stops_restart_through_production_entrypoints() {
    for action in ["park", "restore", "drop"] {
        let baseline = Fixture::new();
        let p = (action != "park").then(|| baseline.park());
        trace(None);
        match action {
            "park" => {
                baseline.park();
            }
            "restore" => bring_back(&baseline.0, p.as_ref().unwrap()).unwrap(),
            _ => {
                drop_to_branch(&baseline.0, p.as_ref().unwrap()).unwrap();
            }
        }
        let boundaries = labels();
        for stop in 1..=boundaries.len() {
            let f = Fixture::new();
            let p = (action != "park").then(|| f.park());
            trace(Some(stop));
            effects::ABRUPT.with(|a| a.set(true));
            let result = std::panic::catch_unwind(|| match action {
                "park" => {
                    park(&f.0, "test-id", &["file".into()]).unwrap();
                }
                "restore" => bring_back(&f.0, p.as_ref().unwrap()).unwrap(),
                _ => {
                    drop_to_branch(&f.0, p.as_ref().unwrap()).unwrap();
                }
            });
            effects::ABRUPT.with(|a| a.set(false));
            trace(None);
            assert!(result.is_err(), "{action}:{stop}");
            f.proof();
            let first = list(&f.0).unwrap();
            assert_eq!(list(&f.0).unwrap(), first);
            if let Some(p) = p.as_ref().or(first.first()) {
                match bring_back(&f.0, p) {
                    Ok(()) => {
                        assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"\xffraw\0\r\n");
                        assert_eq!(
                            std::fs::read(f.0.join(".git/index")).unwrap(),
                            b"DIRC-original"
                        );
                        assert_eq!(
                            std::fs::read(f.0.join(".git/MERGE_HEAD")).unwrap(),
                            b"theirs\r\n"
                        );
                    }
                    Err(e) => {
                        // Interrupted Git/index-lock publication requires explicit recovery;
                        // do not silently ignore restart failure or remove an unowned lock.
                        assert!(
                            e.contains("Interrupted recovery")
                                || e.contains("progress")
                                || e.contains("locked")
                                || e.contains("Commit or stash"),
                            "{action}:{stop}: {e}"
                        );
                    }
                }
                drop_to_branch(&f.0, p).unwrap();
                refs_clean(&f, p);
                for _ in 0..3 {
                    assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
                }
            } else {
                // Archiving stopped before publication: the original live operation is
                // untouched. Explicitly discard this incomplete reservation on restart.
                assert_eq!(action, "park");
                assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"\xffraw\0\r\n");
                assert_eq!(
                    std::fs::read(f.0.join(".git/index")).unwrap(),
                    b"DIRC-original"
                );
                assert_eq!(
                    std::fs::read(f.0.join(".git/MERGE_HEAD")).unwrap(),
                    b"theirs\r\n"
                );
                forget(&f.0, "test-id");
                assert!(
                    run(
                        &f.0,
                        &[
                            "rev-parse",
                            "-q",
                            "--verify",
                            "refs/gitcraft/parked/test-id"
                        ]
                    )
                    .is_err()
                );
                assert!(!folder(&f.0, "test-id").unwrap().exists());
                assert!(list(&f.0).unwrap().is_empty());
            }
            f.proof();
        }
        eprintln!(
            "abrupt {action}: {} production boundaries",
            boundaries.len()
        );
    }
}

#[test]
fn destination_temp_preserves_new_data_and_ignores_preexisting_staging() {
    let f = Fixture::new();
    let p = f.park();
    std::fs::create_dir(f.0.join(".gitcraft-restore-0")).unwrap();
    std::fs::write(f.0.join(".gitcraft-restore-0/bytes"), b"user-owned").unwrap();
    let repo = f.0.clone();
    effects::HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |label| {
            if label == "destination-temp-durable" {
                assert!(repo.join(".gitcraft-restore-1/bytes").is_file());
                std::fs::write(repo.join("file"), b"new-user-data").unwrap();
            }
        }))
    });
    assert!(bring_back(&f.0, &p).is_err());
    effects::HOOK.with(|h| *h.borrow_mut() = None);
    assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"new-user-data");
    assert_eq!(
        std::fs::read(f.0.join(".gitcraft-restore-0/bytes")).unwrap(),
        b"user-owned"
    );
    f.proof();
}

#[cfg(unix)]
#[test]
fn actual_cross_device_raw_publication() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let p = f.park();
    let common = f.0.join(".git/gitcraft");
    let other = Path::new("/dev/shm").join(f.0.file_name().unwrap());
    if !Path::new("/dev/shm").is_dir()
        || std::fs::metadata("/dev/shm").unwrap().dev() == std::fs::metadata(&f.0).unwrap().dev()
    {
        eprintln!("cross-device unavailable: no distinct /dev/shm filesystem");
        return;
    }
    std::fs::create_dir(&other).unwrap();
    // No subprocesses: recursively copy only this fixture's owned archive.
    fn copy(from: &Path, to: &Path) {
        for e in std::fs::read_dir(from).unwrap() {
            let e = e.unwrap();
            let dest = to.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                std::fs::create_dir(&dest).unwrap();
                copy(&e.path(), &dest);
            } else {
                std::fs::copy(e.path(), dest).unwrap();
            }
        }
    }
    copy(&common, &other);
    std::fs::remove_dir_all(&common).unwrap();
    std::os::unix::fs::symlink(&other, &common).unwrap();
    let probe = other.join("cross-device-probe");
    std::fs::write(&probe, b"owned").unwrap();
    assert_eq!(
        std::fs::rename(&probe, f.0.join("probe"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::CrossesDevices
    );
    bring_back(&f.0, &p).unwrap();
    refs_clean(&f, &p);
    std::fs::remove_file(&common).unwrap();
    std::fs::remove_dir_all(&other).unwrap();
}

#[test]
fn ignored_file_on_an_affected_path_blocks_restore_without_changing_bytes() {
    let f = Fixture::new();
    let p = f.park();
    let ignored = f.0.join("new-from-merge");
    std::fs::write(&ignored, b"\xffignored-user-data\0\r\n").unwrap();
    crate::git::added(&f.0, "new-from-merge\0");
    trace(None);
    assert!(bring_back(&f.0, &p).unwrap_err().contains("new-from-merge"));
    assert_eq!(
        std::fs::read(&ignored).unwrap(),
        b"\xffignored-user-data\0\r\n"
    );
    assert!(!labels().iter().any(|s| s.starts_with("git:read-tree")));
    assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"clean");
    assert_eq!(
        std::fs::read(f.0.join(".git/index")).unwrap(),
        b"clean-index"
    );
    assert_eq!(list(&f.0).unwrap(), vec![p.clone()]);
    drop_to_branch(&f.0, &p).unwrap();
    refs_clean(&f, &p);
}

#[cfg(unix)]
#[test]
fn symlink_source_is_a_failed_precondition_and_does_not_publish_a_plan() {
    let f = Fixture::new();
    let p = f.park();
    let dir = folder(&f.0, &p.id).unwrap();
    let raw = dir.join("raw/file");
    std::fs::rename(&raw, dir.join("saved-file")).unwrap();
    std::os::unix::fs::symlink("../saved-file", &raw).unwrap();
    assert!(bring_back(&f.0, &p).is_err());
    assert_eq!(std::fs::read(f.0.join("file")).unwrap(), b"clean");
    assert!(!dir.join("restore-plan.json").exists());
    std::fs::remove_file(&raw).unwrap();
    std::fs::rename(dir.join("saved-file"), raw).unwrap();
    bring_back(&f.0, &p).unwrap();
    refs_clean(&f, &p);
}

#[cfg(unix)]
#[test]
fn rejected_restore_and_drop_release_inherited_descriptors_before_listing() {
    for restore in [false, true] {
        let f = Fixture::new();
        let p = f.park();
        if restore {
            std::fs::write(f.0.join(".git/index.lock"), b"independent git writer").unwrap();
        } else {
            let archive = crate::agent::repo_file(&f.0, "parked-recovery/test-id").unwrap();
            std::fs::create_dir_all(&archive).unwrap();
            std::fs::write(archive.join("existing"), b"mine").unwrap();
        }
        let inherited = locking::tests::Inherited::new();
        let error = if restore {
            bring_back(&f.0, &p).unwrap_err()
        } else {
            drop_to_branch(&f.0, &p).unwrap_err()
        };
        assert!(!error.contains("is busy"), "{error}");
        assert_eq!(inherited.count(), Ok(1));
        trace(None);
        assert_eq!(list(&f.0).unwrap(), vec![p]);
        assert!(
            matches!(inherited.count(), Ok(n) if n > 1),
            "duplicates remain open through listing"
        );
        f.proof();
    }
}
