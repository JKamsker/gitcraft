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
