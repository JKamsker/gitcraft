//! One stable OS lock per common-dir/archive id, outside both movable archives.
//! Lock files are permanent: unlinking a held inode would split ownership.
use super::*;

pub(super) struct Lease {
    file: std::fs::File,
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
    #[cfg(all(test, unix))]
    tests::retain(&file)?;
    Ok(Lease { file })
}

impl Drop for Lease {
    fn drop(&mut self) {
        // flock belongs to the open file description. A concurrent fork can inherit
        // it until exec, even with CLOEXEC; closing only our descriptor then leaves
        // the completed operation locked. End ownership explicitly, including on
        // early returns and unwinding. Lease is private and never cloned or handed
        // to children: inherited descriptors do not own a recovery operation.
        let _ = self.file.unlock();
        // Keep the permanent inode; unlinking would split independent callers.
    }
}

#[cfg(all(test, unix))]
#[path = "locking/tests.rs"]
pub(super) mod tests;
