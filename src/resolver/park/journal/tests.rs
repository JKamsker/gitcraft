use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("gitcraft-journal-{}-{n}", std::process::id()));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn phases() -> Vec<Phase> {
    vec![
        Phase::Archiving,
        Phase::Archived,
        Phase::Clearing,
        Phase::Parked,
        Phase::Restoring,
        Phase::Restored,
        Phase::Cleanup,
        Phase::Dropping {
            branch: "parked/merge-recording".into(),
        },
    ]
}

#[test]
fn recording_interruption_before_and_after_every_transition_preserves_bytes_or_archive()
-> Result<(), Box<dyn std::error::Error>> {
    // No Git or child process is launched. Effects are recorded against synthetic byte
    // snapshots, including bytes that a clean filter would conceal, index and merge state.
    let originals = [
        b"\xff\0raw\r\n".as_slice(),
        b"DIRC\0index",
        b"metadata\r\n",
        b"ignored\xff",
    ];
    for (stop, _) in phases().iter().enumerate() {
        for after in [false, true] {
            let fixture = Fixture::new()?;
            let archive = fixture.0.join("archive");
            std::fs::create_dir(&archive)?;
            let mut live = originals.map(Vec::from);
            for (n, phase) in phases().into_iter().enumerate() {
                if n == stop && !after {
                    break;
                }
                record(&archive, phase.clone())?;
                match phase {
                    Phase::Archiving => {
                        for (n, bytes) in originals.iter().enumerate().take(3) {
                            std::fs::write(archive.join(n.to_string()), bytes)?;
                        }
                        sync_tree(&archive)?;
                    }
                    Phase::Clearing => {
                        for file in &mut live[..3] {
                            *file = b"clean".to_vec();
                        }
                    }
                    Phase::Restoring => {
                        for (n, file) in live.iter_mut().enumerate().take(3) {
                            *file = std::fs::read(archive.join(n.to_string()))?;
                        }
                    }
                    _ => {}
                }
                if n == stop {
                    break;
                }
            }
            assert_eq!(
                live[3], originals[3],
                "ignored bytes are never a transition target"
            );
            for (n, bytes) in originals.iter().enumerate().take(3) {
                assert!(
                    live[n] == *bytes || std::fs::read(archive.join(n.to_string()))? == *bytes,
                    "transition {stop}, after={after}, file {n}"
                );
            }
            // A fresh reader sees the last durable receipt; repeated reads do not mutate it.
            let before = latest(&archive)?;
            assert_eq!(latest(&archive)?, before);
        }
    }
    Ok(())
}

#[test]
fn torn_and_unknown_receipts_fail_closed_without_discarding_the_archive()
-> Result<(), Box<dyn std::error::Error>> {
    for bad in [b"{".as_slice(), br#"{"version":99,"phase":"Clearing"}"#] {
        let fixture = Fixture::new()?;
        std::fs::write(fixture.0.join("raw"), b"\xffraw")?;
        record(&fixture.0, Phase::Archived)?;
        let path = fixture.0.join("journal-v1/00000001.json");
        std::fs::write(&path, bad)?;
        assert!(latest(&fixture.0).is_err());
        assert!(record(&fixture.0, Phase::Clearing).is_err());
        assert_eq!(std::fs::read(fixture.0.join("raw"))?, b"\xffraw");
        assert_eq!(std::fs::read(path)?, bad);
    }
    Ok(())
}

#[test]
fn legacy_archive_reads_without_rewriting_and_receipts_round_trip()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    assert_eq!(latest(&fixture.0)?, None);
    assert!(!fixture.0.join("journal-v1").exists());
    for phase in phases() {
        record(&fixture.0, phase.clone())?;
        assert_eq!(latest(&fixture.0)?, Some(phase));
    }
    Ok(())
}

#[test]
fn cleanup_restart_is_idempotent_and_archive_collisions_preserve_both_copies()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let source = fixture.0.join("source");
    let destination = fixture.0.join("recovery");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("raw"), b"\xfforiginal\0")?;
    record(&source, Phase::Restored)?;
    // Restart after reserving the destination but before moving the archive.
    std::fs::create_dir(&destination)?;
    retire_paths(&source, &destination)?;
    retire_paths(&source, &destination)?;
    assert_eq!(
        std::fs::read(destination.join("parked/raw"))?,
        b"\xfforiginal\0"
    );
    assert_eq!(latest(&destination.join("parked"))?, Some(Phase::Restored));
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("raw"), b"new source")?;
    assert!(retire_paths(&source, &destination).is_err());
    assert_eq!(std::fs::read(source.join("raw"))?, b"new source");
    assert_eq!(
        std::fs::read(destination.join("parked/raw"))?,
        b"\xfforiginal\0"
    );
    Ok(())
}

#[test]
fn existing_file_sync_requests_windows_write_access_without_changing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let file = fixture.0.join("raw-index");
    let original = b"DIRC\0raw\xff\r\n";
    std::fs::write(&file, original)?;
    // A zero-byte write checks the actual handle's write access without altering
    // bytes. Model FlushFileBuffers' documented GENERIC_WRITE requirement on Unix
    // too, so a read-only-open regression fails before any native Windows run.
    let windows_flush = |mut handle: &std::fs::File| -> std::io::Result<()> {
        use std::io::Write;
        assert_eq!(handle.write(&[])?, 0);
        handle.sync_all()
    };
    #[cfg(unix)]
    assert!(sync_existing(&file, false, windows_flush).is_err());
    sync_existing(&file, true, windows_flush)?;
    sync_file(&file)?;
    sync_tree(&fixture.0)?;
    assert_eq!(std::fs::read(&file)?, original);
    let missing = fixture.0.join("missing");
    let mut flushed = false;
    let error = sync_existing(&missing, true, |_| {
        flushed = true;
        Ok(())
    })
    .err()
    .ok_or("Expected missing recovery file error")?;
    assert!(!flushed && !missing.exists());
    assert!(
        error.contains("Open recovery file") && error.contains("missing"),
        "{error}"
    );
    Ok(())
}

#[test]
fn failed_regular_file_flush_reports_its_path_and_retains_originals()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let file = fixture.0.join("original-index");
    std::fs::write(&file, b"original")?;
    let error = sync_existing(&file, true, |_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "recorded FlushFileBuffers failure",
        ))
    })
    .err()
    .ok_or("Expected recorded flush failure")?;
    assert!(
        error.contains("Sync recovery file") && error.contains("original-index"),
        "{error}"
    );
    assert!(
        error.contains("recorded FlushFileBuffers failure"),
        "{error}"
    );
    assert_eq!(std::fs::read(file)?, b"original");
    Ok(())
}
