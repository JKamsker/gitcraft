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

/// What [`list`](super::list) finds: archives, and reservations of put-asides that never
/// completed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub parked: Vec<Parked>,
    /// Ids and folders of reservations interrupted before their manifest was published (or
    /// with one torn while still archiving). Their worktree was never cleared, so nothing of
    /// them is lost; [`forget`](super::forget) removes one and its ref. Never removed
    /// automatically.
    pub incomplete: Vec<(String, PathBuf)>,
}

/// Active parked merges, retained completed recovery archives (oldest first) and
/// incomplete reservations for `repo`.
pub(super) fn listing(repo: &Path) -> Result<Listing, String> {
    let top = trimmed(run(repo, &["rev-parse", "--show-toplevel"])?);
    let ids = identities(repo)?;
    // A snapshot can name a source which is retired before its lock is acquired.
    // Resolve and read under the same stable lease used by foreground operations.
    let mut found = Listing::default();
    for id in ids {
        let _lease = locking::acquire(repo, &id)?;
        let parked = match manifest(repo, &id)? {
            Manifest::Archive(parked) => parked,
            Manifest::Incomplete(dir) => {
                found.incomplete.push((id, dir));
                continue;
            }
            Manifest::Absent => continue,
        };
        if crate::remote::same_dir(Path::new(&parked.worktree), Path::new(&top))
            && journal::recover_locked(repo, &parked)?
        {
            found.parked.push(parked);
        }
    }
    found.parked.sort_by(|a, b| a.id.cmp(&b.id));
    found.parked.dedup_by(|a, b| a == b);
    found.parked.sort_by_key(|p| p.at);
    Ok(found)
}

pub(super) enum Manifest {
    Archive(Parked),
    /// An active reservation whose manifest is missing or torn while its last receipt is
    /// still Archiving (or it has none): the worktree was never cleared.
    Incomplete(PathBuf),
    /// Nothing readable where a retained archive would be.
    Absent,
}

/// The caller holds the stable identity lease through reading and reconciliation.
pub(super) fn manifest(repo: &Path, id: &str) -> Result<Manifest, String> {
    let source = folder(repo, id)?;
    let retained = crate::agent::repo_file(repo, &format!("parked-recovery/{id}/parked"))?;
    if source.join("parked.json").exists() && retained.join("parked.json").exists() {
        return Err(format!("Recovery archive collision: {id}"));
    }
    let dir = if source.join("parked.json").exists() {
        source.clone()
    } else if retained.is_dir() {
        retained
    } else {
        source.clone()
    };
    let incomplete = || {
        if dir == source {
            Manifest::Incomplete(dir.clone())
        } else {
            Manifest::Absent
        }
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
                return Ok(incomplete());
            }
            return Err(format!("Missing recovery manifest: {}", path.display()));
        }
        Err(e) => return Err(format!("Read {}: {e}", path.display())),
    };
    let parked: Parked = match serde_json::from_slice(&bytes) {
        Ok(parked) => parked,
        // While the last receipt is Archiving the worktree was never cleared, so an
        // unreadable manifest (torn by a version that wrote it in place) is an incomplete
        // reservation, not an archive: it must not hide every other archive of the
        // repository. It stays on disk, untouched, and is reported. Without receipts (a
        // legacy archive, whose worktree may have been cleared) or past Archiving it is
        // corruption: fail closed.
        Err(e) if journal::latest(&dir)? == Some(journal::Phase::Archiving) => {
            effects::note(&format!(
                "Incomplete recovery reservation {} (unreadable manifest: {e})",
                dir.display()
            ));
            return Ok(incomplete());
        }
        Err(e) => return Err(format!("Invalid recovery manifest {}: {e}", path.display())),
    };
    if parked.id != id {
        return Err(format!("Recovery manifest identity mismatch: {id}"));
    }
    Ok(Manifest::Archive(parked))
}
