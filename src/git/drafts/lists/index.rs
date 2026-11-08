//! Own only index.lock. Exchanges retain the old inode until the native ref transaction
//! is acknowledged. Concurrent non-cooperating changes retain their bytes and our backup.
use super::*;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) struct Index {
    path: std::path::PathBuf,
    lock: std::path::PathBuf,
    before: Vec<u8>,
    after: Vec<u8>,
    before_id: (u64, u64),
    after_id: (u64, u64),
    exchanged: bool,
    keep: bool,
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Index {
    fn identity(path: &Path) -> Result<(u64, u64), String> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !m.is_file() || m.file_type().is_symlink() {
            return Err("Index exchange requires a regular owned file".into());
        }
        Ok((m.dev(), m.ino()))
    }
    fn matches(path: &Path, token: (u64, u64), bytes: &[u8]) -> bool {
        Self::identity(path).ok() == Some(token)
            && path
                .parent()
                .zip(path.file_name().and_then(|s| s.to_str()))
                .and_then(|(p, n)| crate::safe_read::read_inside(p, n, 32 * 1024 * 1024))
                .as_deref()
                == Some(bytes)
    }
    pub fn new(ctx: &Context, before: &Source, after: Vec<u8>) -> Result<Self, String> {
        use std::io::Write;
        if remote::is_remote(&ctx.repo) {
            return Err(
                "Native changelist publication is required; remote index was not changed".into(),
            );
        }
        let path = std::path::PathBuf::from(&ctx.index_file);
        let parent = path.parent().ok_or("Missing index parent")?;
        if std::fs::canonicalize(parent).map_err(|e| e.to_string())? != parent {
            return Err("Changelist index parent must be a physical native directory".into());
        }
        let before_id = Self::identity(&path)?;
        let lock = path.with_file_name(format!(
            "{}.lock",
            path.file_name()
                .ok_or("Missing index name")?
                .to_str()
                .ok_or("Index path must be UTF-8")?
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
            .map_err(|_| "Index is locked; finish the other Git operation first")?;
        let after_id = Self::identity(&lock)?;
        let own = Self {
            path,
            lock,
            before: before.cache.clone(),
            after,
            before_id,
            after_id,
            exchanged: false,
            keep: false,
        };
        file.write_all(&own.after)
            .and_then(|_| file.set_permissions(std::fs::metadata(&own.path)?.permissions()))
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        let mut own = own;
        if !Self::matches(&own.path, own.before_id, &own.before) {
            return Err("Index changed before publication".into());
        }
        own.swap()?;
        own.exchanged = true;
        if !Self::matches(&own.lock, own.before_id, &own.before) {
            return Err("Index changed at the exchange boundary".into());
        }
        Ok(own)
    }
    fn swap(&self) -> Result<(), String> {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        renameat_with(CWD, &self.lock, CWD, &self.path, RenameFlags::EXCHANGE)
            .map_err(|e| e.to_string())
    }
    pub fn accept(&mut self) {
        self.keep = true;
        if Self::matches(&self.lock, self.before_id, &self.before) {
            let _ = std::fs::remove_file(&self.lock);
        }
    }
    pub fn uncertain(&mut self) {
        self.keep = true;
    }
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for Index {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        if self.exchanged {
            if !Self::matches(&self.path, self.after_id, &self.after) || self.swap().is_err() {
                return;
            }
        }
        if Self::matches(&self.lock, self.after_id, &self.after) {
            let _ = std::fs::remove_file(&self.lock);
        }
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) struct Index;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl Index {
    pub fn new(_: &Context, _: &Source, _: Vec<u8>) -> Result<Self, String> {
        Err("Native atomic changelist index publication is unavailable on this platform; no files changed".into())
    }
    pub fn accept(&mut self) {}
    pub fn uncertain(&mut self) {}
}
