//! One stable OS lock per common-dir/archive id, outside both movable archives.
//! Lock files are permanent: unlinking a held inode would split ownership.
use super::*;

pub(super) struct Lease {
    // Closing the retained handle releases the lock; it is never read directly.
    _file: std::fs::File,
}

pub(super) fn acquire(repo: &Path, id: &str) -> Result<Lease, String> {
    if id.is_empty()
        || Path::new(id).components().count() != 1
        || !matches!(
            Path::new(id).components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err("Invalid recovery id".into());
    }
    let path = crate::agent::repo_file(repo, &format!("parked-locks/{id}"))?;
    std::fs::create_dir_all(path.parent().ok_or("Missing lock parent")?)
        .map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.is_symlink()) {
        return Err("Invalid recovery lock file".into());
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.try_lock()
        .map_err(|e| format!("Recovery {id} is busy: {e}"))?;
    Ok(Lease { _file: file }) // Drop closes this handle and releases the OS lock, even on panic.
}
