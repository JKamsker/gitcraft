//! Append-only recovery receipts. A receipt is durable before its destructive effect.
//! Recovery never resets a worktree: uncertain work stays discoverable in the archive.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Phase {
    Archiving,
    Archived,
    Clearing,
    Parked,
    Restoring,
    Restored,
    Cleanup,
    Dropping { branch: String },
    Uncertain,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    version: u32,
    phase: Phase,
}

pub(super) fn sync_dir(dir: &Path) -> Result<(), String> {
    // Windows does not support opening directories through std::fs::File.
    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("sync {}: {e}", dir.display()))?;
    let _ = dir;
    Ok(())
}

// FlushFileBuffers requires GENERIC_WRITE on Windows, including for an existing
// archive or live file. Unix permits fsync on a read-only descriptor. Never create,
// truncate, change permissions, or skip a failed flush to obtain durability.
fn sync_existing(
    path: &Path,
    windows: bool,
    flush: impl FnOnce(&std::fs::File) -> std::io::Result<()>,
) -> Result<(), String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(windows)
        .open(path)
        .map_err(|e| format!("Open recovery file {} for sync: {e}", path.display()))?;
    flush(&file).map_err(|e| format!("Sync recovery file {}: {e}", path.display()))
}

pub(super) fn sync_file(path: &Path) -> Result<(), String> {
    sync_existing(path, cfg!(windows), std::fs::File::sync_all)?;
    super::effects::boundary("file-synced")
}

pub(super) fn sync_tree(dir: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            sync_tree(&entry.path())?;
        } else if kind.is_file() {
            sync_existing(&entry.path(), cfg!(windows), std::fs::File::sync_all)?;
        } else {
            return Err("Recovery archive contains a non-regular file".into());
        }
    }
    sync_dir(dir)
}

#[path = "journal/receipts.rs"]
mod receipts;
pub(super) use receipts::{atomic_write, latest, record};

/// Persist reachability, including ancestors created by repo_file/reserve.
pub(super) fn sync_ancestors(path: &Path) -> Result<(), String> {
    let mut at = Some(path);
    while let Some(dir) = at {
        sync_dir(dir)?;
        super::effects::boundary("directory-synced")?;
        at = dir.parent();
    }
    Ok(())
}

/// Finish only archive movement after a proven restore. Repeated calls are harmless.
/// Raw files, original index and metadata are retained even after successful restoration.
pub(super) fn retire(repo: &Path, id: &str) -> Result<(), String> {
    let source = folder(repo, id)?;
    let destination = crate::agent::repo_file(repo, &format!("parked-recovery/{id}"))?;
    let authoritative = super::discovery::locate(repo, id)?;
    let metadata = authoritative.join("parked.json");
    let parked: Parked =
        serde_json::from_slice(&std::fs::read(metadata).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let recovery_ref = format!("refs/gitcraft/parked-recovery/{id}");
    match run(repo, &["rev-parse", "-q", "--verify", &recovery_ref]) {
        Ok(tip) if tip.trim_end_matches(['\n', '\r']) == parked.snapshot => {}
        Ok(_) => return Err(format!("Recovery ref collision: {recovery_ref}")),
        Err(_) => {
            run(repo, &["update-ref", &recovery_ref, &parked.snapshot, ""])?;
        }
    }
    if authoritative == source {
        retire_paths(&source, &destination)?;
    } else {
        sync_ancestors(&destination)?;
        sync_dir(source.parent().ok_or("Missing source parent")?)?;
    }
    match run(repo, &["rev-parse", "-q", "--verify", &parked.ref_name()]) {
        Ok(tip) if tip.trim_end_matches(['\n', '\r']) == parked.snapshot => {
            run(
                repo,
                &["update-ref", "-d", &parked.ref_name(), &parked.snapshot],
            )?;
        }
        Ok(_) => return Err("Original parking ref moved; both archives retained".into()),
        Err(_) => {} // Already removed by a previous cleanup attempt.
    }
    Ok(())
}

pub(super) fn retire_paths(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() && destination.join("parked").is_dir() {
        sync_ancestors(destination)?;
        if let Some(parent) = source.parent() {
            sync_dir(parent)?;
        }
        return Ok(());
    }
    match std::fs::create_dir(destination) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            reserve(destination)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // An empty reservation can remain after interruption before rename. Never
            // merge with a nonempty archive, even when it has the same identifier.
            if std::fs::read_dir(destination)
                .map_err(|e| e.to_string())?
                .next()
                .is_some()
            {
                return Err(format!(
                    "Recovery archive collision: {}",
                    destination.display()
                ));
            }
        }
        Err(e) => return Err(e.to_string()),
    }
    if destination.join("parked").exists() {
        return Err(format!(
            "Recovery archive collision: {}",
            destination.display()
        ));
    }
    std::fs::rename(source, destination.join("parked")).map_err(|e| e.to_string())?;
    super::effects::boundary("archive-moved")?;
    sync_ancestors(destination)?;
    if let Some(parent) = destination.parent() {
        sync_dir(parent)?;
    }
    if let Some(parent) = source.parent() {
        sync_dir(parent)?;
    }
    // The caller pins snapshot objects under the recovery ref before moving the archive.
    Ok(())
}

/// Reconcile moved archives too; never infer live restoration from a receipt alone.
pub(super) fn recover_locked(repo: &Path, parked: &Parked) -> Result<bool, String> {
    let source = folder(repo, &parked.id)?;
    let dir = super::discovery::locate(repo, &parked.id)?;
    if dir != source {
        retire(repo, &parked.id)?;
        return Ok(true); // Retained archives remain discoverable and actionable.
    }
    match latest(&dir)? {
        Some(Phase::Restored | Phase::Cleanup) => {
            let gd = git_dir(repo)?;
            if super::restore::verified(repo, &gd, &dir, parked)? {
                super::restore::sync_live(repo, &gd, parked)?;
                record(&dir, Phase::Restored)?;
                retire(repo, &parked.id)?;
            }
            Ok(true)
        }
        _ => Ok(true),
    }
}

#[cfg(test)]
#[path = "journal/tests.rs"]
mod tests;
