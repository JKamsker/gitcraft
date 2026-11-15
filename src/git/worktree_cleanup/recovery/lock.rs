//! Cooperative cross-process exclusion. The sibling lock is persistent: unlinking it
//! would let an old waiter and a new opener lock different inodes for the same item.
//! This advisory lock excludes cooperating Gitcraft operations, not external renames.
//! In particular, a Unix fd keeps referring to a directory moved out of recovery.
use std::fs::File;
use std::path::Path;

/// Owns the operation, independently of transiently inherited/duplicated descriptors.
#[derive(Debug)]
pub(super) struct Operation(File);
impl Drop for Operation {
    fn drop(&mut self) {
        // flock is attached to the open-file description. Closing only our descriptor
        // can leave it held by a child forked before CLOEXEC took effect. The operation
        // has ended, so release it explicitly before File closes its descriptor.
        let _ = self.0.unlock();
    }
}

pub(super) fn acquire(root: &Path, directory: &Path) -> Result<Operation, String> {
    let name = directory.file_name().ok_or("Missing recovery name")?;
    if directory.parent() != Some(root) || !name.to_string_lossy().starts_with("tree-") {
        return Err("Invalid recovery item".into());
    }
    let mut name = name.to_owned();
    name.push(".operation.lock");
    let path = root.join(name);
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        File::from(
            open(
                path,
                OFlags::RDWR | OFlags::CREATE | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|e| e.to_string())?,
        )
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0x1 | 0x2)
            .custom_flags(0x0020_0000)
            .open(path)
            .map_err(|e| e.to_string())?
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("Recovery lock must be a plain file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Recovery lock must not be a reparse point".into());
        }
    }
    file.try_lock()
        .map_err(|e| format!("Recovery operation already running or lock unavailable: {e:?}"))?;
    Ok(Operation(file))
}

#[cfg(all(test, unix))]
mod tests {
    /// Model an inherited open-file description without spawning or signalling a process.
    #[test]
    fn owner_drop_releases_operation_even_with_a_duplicated_descriptor()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let item = temp.path().join("tree-duplicate");
        let held = super::acquire(temp.path(), &item)?;
        let inherited = held.0.try_clone()?;
        drop(held);
        assert!(super::acquire(temp.path(), &item).is_ok());
        drop(inherited);
        Ok(())
    }
}
