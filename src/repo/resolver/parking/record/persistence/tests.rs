use super::*;

#[test]
fn interrupted_publication_recovers_from_versioned_intent_and_rejects_future_versions()
-> Result<(), Box<dyn std::error::Error>> {
    let repo = std::env::temp_dir().join(format!("gitcraft-run-receipts-{}", std::process::id()));
    std::fs::create_dir(&repo)?;
    let record = RunRecord {
        engine: "Codex".into(),
        ..RunRecord::default()
    };
    for version in [1, 2] {
        let intent =
            serde_json::to_vec(&serde_json::json!({"version": version, "record": record}))?;
        receipt(&repo, "agent-archive-v1.json", &intent)?;
        assert_eq!(read_record(&repo).is_some(), version == 1);
    }
    receipt(&repo, "run.json", &serde_json::to_vec(&record)?)?;
    assert_eq!(
        read_record(&repo).ok_or("expected a run record")?.engine,
        "Codex"
    );
    // A partial temporary publication is not consumed in place of the complete record.
    std::fs::write(repo.join(".run.json-interrupted"), b"{")?;
    assert_eq!(
        read_record(&repo).ok_or("expected a run record")?.engine,
        "Codex"
    );
    std::fs::remove_dir_all(repo)?;
    Ok(())
}

#[test]
fn recording_windows_archive_flush_requires_writable_nontruncating_handle()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Seek, Write};
    let dir = std::env::temp_dir().join(format!("gitcraft-archive-access-{}", std::process::id()));
    std::fs::create_dir(&dir)?;
    let path = dir.join("workspace.json");
    let bytes = b"archive contents";
    std::fs::write(&path, bytes)?;
    let require_write = |mut file: &std::fs::File| {
        // Model the Windows flush access requirement on the actual opened handle. A
        // read-only File::open (the old code) cannot write even on Linux.
        let mut existing = Vec::new();
        file.read_to_end(&mut existing)?;
        assert_eq!(
            existing, bytes,
            "opening must not truncate archive contents"
        );
        file.rewind()?;
        file.write_all(&existing)?;
        file.sync_all()
    };
    assert!(
        sync_archive_file(&path, false, require_write).is_err(),
        "negative control"
    );
    sync_archive_file(&path, true, require_write)?;
    // Native acceptance also exercises the real platform flush, without the model.
    #[cfg(windows)]
    assert!(std::fs::File::open(&path)?.sync_all().is_err());
    sync_tree(&dir)?;
    assert_eq!(std::fs::read(&path)?, bytes);
    let error = sync_archive_file(&path, true, |_| Err(std::io::Error::other("flush failed")))
        .err()
        .ok_or("missing sync failure")?;
    assert!(error.contains("workspace.json") && error.contains("flush failed"));
    assert!(sync_archive_file(&dir.join("missing"), true, |_| Ok(())).is_err());
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
