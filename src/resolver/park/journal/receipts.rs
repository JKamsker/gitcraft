use super::*;

fn entries(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let journal = dir.join("journal-v1");
    if !journal.exists() {
        return Ok(Vec::new());
    }
    let mut paths = std::fs::read_dir(journal)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|p| p.extension().is_some_and(|e| e == "json"));
    paths.sort();
    Ok(paths)
}

/// Temp files are never receipts. A failed write can be retried without truncating
/// an acknowledged receipt. Callers serialize mutations of one archive.
pub(in crate::resolver::park) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("pending");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    super::super::effects::boundary("atomic-temp-synced")?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())?;
    super::super::effects::boundary("atomic-renamed")?;
    sync_dir(path.parent().ok_or("Missing parent")?)?;
    super::super::effects::boundary("atomic-parent-synced")
}

pub(in crate::resolver::park) fn latest(dir: &Path) -> Result<Option<Phase>, String> {
    let paths = entries(dir)?;
    let mut latest = None;
    for (n, path) in paths.iter().enumerate() {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let receipt = match serde_json::from_slice::<Receipt>(&bytes) {
            Ok(receipt) => receipt,
            Err(_) => {
                // Only the final legacy receipt may have been torn by create+write.
                // Preserve its evidence, replace it with an explicit uncertain state.
                // Unknown versions and interior corruption always fail closed.
                let value = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
                if n + 1 != paths.len()
                    || value.is_some()
                    || !bytes.starts_with(br#"{"version":1,"#)
                {
                    return Err(format!("Ambiguous recovery receipt: {}", path.display()));
                }
                std::fs::copy(path, path.with_extension("torn")).map_err(|e| e.to_string())?;
                sync_tree(&dir.join("journal-v1"))?;
                let repaired = serde_json::to_vec(&Receipt {
                    version: 2,
                    phase: Phase::Uncertain,
                })
                .map_err(|e| e.to_string())?;
                atomic_write(path, &repaired)?;
                Receipt {
                    version: 2,
                    phase: Phase::Uncertain,
                }
            }
        };
        if !matches!(receipt.version, 1 | 2) {
            return Err(format!(
                "Unsupported parking journal version {}",
                receipt.version
            ));
        }
        latest = Some(receipt.phase);
    }
    Ok(latest)
}

pub(in crate::resolver::park) fn record(dir: &Path, phase: Phase) -> Result<(), String> {
    latest(dir)?;
    let count = entries(dir)?.len();
    let journal = dir.join("journal-v1");
    // Never resurrect an authoritative directory moved by a previous owner.
    match std::fs::create_dir(&journal) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && journal.is_dir() => {}
        Err(e) => return Err(e.to_string()),
    }
    sync_ancestors(&journal)?;
    let label = format!("phase:{phase:?}");
    let bytes = serde_json::to_vec(&Receipt { version: 2, phase }).map_err(|e| e.to_string())?;
    atomic_write(&journal.join(format!("{count:08}.json")), &bytes)?;
    super::super::effects::boundary("receipt-durable")?;
    super::super::effects::boundary(&label)
}
