//! Agent results are published only after a complete, durable workspace exists. The
//! original scratch is preserved before copying, so an interrupted copy remains recoverable.
use super::*;
use std::io::Write as _;

fn sync_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    let _ = dir;
    Ok(())
}
fn sync_tree(dir: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            sync_tree(&entry.path())?;
        } else {
            std::fs::File::open(entry.path())
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
    }
    sync_dir(dir)
}

pub(super) fn keep_record(
    ws: &Workspace,
    dir: &Path,
    record: &RunRecord,
) -> Result<Workspace, String> {
    ws.preserve()?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Versioned intent contains the original scratch location, even if run.json is absent.
    let intent = serde_json::to_vec(&serde_json::json!({
        "version": 1, "phase": "archiving", "source": ws.dir(), "record": record,
    }))
    .map_err(|e| e.to_string())?;
    receipt(dir, "agent-archive-v1.json", &intent)?;
    let kept = ws.keep(dir)?;
    sync_tree(dir)?;
    let text = serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?;
    // Existing run.json remains readable if publication is interrupted. The unique temp
    // file is retained on failure and a restart can use the versioned intent's record.
    receipt(dir, "run.json", &text)?;
    Ok(kept)
}

fn receipt(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = dir.join(format!(".{name}-{}-{n}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&temp, dir.join(name)).map_err(|e| e.to_string())?;
    sync_dir(dir)
}

/// A completed workspace with an interrupted run.json publication can be reopened using
/// the intent. Workspace::load still validates every recorded original/result file first.
pub(super) fn read_record(dir: &Path) -> Option<RunRecord> {
    let published = std::fs::read(dir.join("run.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if published.is_some() {
        return published;
    }
    #[derive(Deserialize)]
    struct Intent {
        version: u32,
        record: RunRecord,
    }
    let bytes = std::fs::read(dir.join("agent-archive-v1.json")).ok()?;
    let intent: Intent = serde_json::from_slice(&bytes).ok()?;
    (intent.version == 1).then_some(intent.record)
}

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
