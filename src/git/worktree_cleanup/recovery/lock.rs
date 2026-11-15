//! Cooperative cross-process exclusion. The sibling lock is persistent: unlinking it
//! would let an old waiter and a new opener lock different inodes for the same item.
//! This advisory lock excludes cooperating Gitcraft operations, not external renames.
//! In particular, a Unix fd keeps referring to a directory moved out of recovery.
use std::fs::File;
use std::path::Path;

pub(super) fn acquire(root: &Path, directory: &Path) -> Result<File, String> {
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
    Ok(file)
}
