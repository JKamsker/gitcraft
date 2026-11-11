use super::*;

pub(super) fn locate(repo: &Path, id: &str) -> Result<PathBuf, String> {
    let source = folder(repo, id)?;
    if source.join("parked.json").is_file() {
        return Ok(source);
    }
    let recovery = crate::agent::repo_file(repo, &format!("parked-recovery/{id}/parked"))?;
    if recovery.is_dir() {
        return Ok(recovery);
    }
    Err(format!("Recovery archive {id} is missing"))
}

pub(super) fn archives(repo: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for (root, nested) in [("parked", false), ("parked-recovery", true)] {
        let root = crate::agent::repo_file(repo, root)?;
        match std::fs::read_dir(root) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    paths.push(if nested {
                        entry.path().join("parked")
                    } else {
                        entry.path()
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(paths)
}

/// Byte-exact index, operation metadata and raw files, including retained completions.
pub fn recovery_location(repo: &Path, parked: &Parked) -> Result<PathBuf, String> {
    let dir = locate(repo, &parked.id)?;
    transaction::identity(&dir, parked)?;
    Ok(dir)
}

/// Paths are transient; archive ids and their locks survive retirement.
pub(super) fn identities(repo: &Path) -> Result<std::collections::BTreeSet<String>, String> {
    let mut ids = std::collections::BTreeSet::new();
    let active = crate::agent::repo_file(repo, "parked")?;
    for path in archives(repo)? {
        let id = if path.parent() == Some(active.as_path()) {
            path.file_name()
        } else {
            path.parent().and_then(Path::file_name)
        }
        .and_then(|n| n.to_str())
        .ok_or("Invalid recovery archive id")?;
        ids.insert(id.to_owned());
    }
    #[cfg(recovery_recording)]
    effects::boundary("discovery-snapshot")?;
    Ok(ids)
}

/// The caller holds the stable identity lease through reading and reconciliation.
pub(super) fn manifest(repo: &Path, id: &str) -> Result<Option<Parked>, String> {
    let source = folder(repo, id)?;
    let retained = crate::agent::repo_file(repo, &format!("parked-recovery/{id}/parked"))?;
    if source.join("parked.json").exists() && retained.join("parked.json").exists() {
        return Err(format!("Recovery archive collision: {id}"));
    }
    let dir = if source.join("parked.json").exists() {
        source
    } else if retained.is_dir() {
        retained
    } else {
        source
    };
    let path = dir.join("parked.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // A reservation interrupted before manifest publication is not an archive.
            // A completed archive without its manifest is corruption, never an empty list.
            if matches!(
                journal::latest(&dir)?,
                None | Some(journal::Phase::Archiving)
            ) {
                return Ok(None);
            }
            return Err(format!("Missing recovery manifest: {}", path.display()));
        }
        Err(e) => return Err(format!("Read {}: {e}", path.display())),
    };
    let parked: Parked = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Invalid recovery manifest {}: {e}", path.display()))?;
    if parked.id != id {
        return Err(format!("Recovery manifest identity mismatch: {id}"));
    }
    Ok(Some(parked))
}
