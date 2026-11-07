//! Bounded native-file snapshots. Display strings are never reused as filesystem paths.
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) const FILE_LIMIT: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    readonly: bool,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    mode: u32,
}
impl Stamp {
    pub fn new(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            readonly: metadata.permissions().readonly(),
            #[cfg(unix)]
            dev: metadata.dev(),
            #[cfg(unix)]
            ino: metadata.ino(),
            #[cfg(unix)]
            mode: metadata.mode(),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub stamp: Option<Stamp>,
}
impl Snapshot {
    pub fn read(path: &Path, active: &dyn Fn() -> bool) -> Result<Self, String> {
        if !active() {
            return Err("File comparison cancelled".into());
        }
        if null_input(path) {
            return Ok(Self {
                path: path.into(),
                bytes: Vec::new(),
                stamp: None,
            });
        }
        let file = open_input(path).map_err(|e| format!("Could not open {}: {e}", label(path)))?;
        Self::from_file(path, file, active)
    }
    pub fn from_file(
        path: &Path,
        mut file: File,
        active: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !before.is_file() || before.len() > FILE_LIMIT {
            return Err(format!(
                "{} must be a regular file within 2 MiB",
                label(path)
            ));
        }
        let stamp = Stamp::new(&before);
        let mut bytes = Vec::new();
        let mut block = [0u8; 32 * 1024];
        loop {
            if !active() {
                return Err("File comparison cancelled".into());
            }
            let count = file.read(&mut block).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            if bytes.len() + count > FILE_LIMIT as usize {
                return Err("File comparison is limited to 2 MiB per file".into());
            }
            bytes.extend_from_slice(&block[..count]);
        }
        if Stamp::new(&file.metadata().map_err(|e| e.to_string())?) != stamp
            || bytes.len() as u64 != stamp.len
        {
            return Err(format!(
                "{} changed while reading; reopen the file tool",
                label(path)
            ));
        }
        Ok(Self {
            path: path.into(),
            bytes,
            stamp: Some(stamp),
        })
    }
    pub fn unchanged(&self) -> Result<bool, String> {
        let now = Self::read(&self.path, &|| true)?;
        Ok(now.stamp == self.stamp && now.bytes == self.bytes)
    }
}

#[cfg(unix)]
fn open_input(path: &Path) -> std::io::Result<File> {
    use rustix::fs::{Mode, OFlags, open};
    // A FIFO/device supplied as an ordinary filename never blocks the worker.
    open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(std::io::Error::from)
}
#[cfg(not(unix))]
fn open_input(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

fn null_input(path: &Path) -> bool {
    path == Path::new("/dev/null")
        || (cfg!(windows)
            && path
                .as_os_str()
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("NUL")))
}
pub(super) fn label(path: &Path) -> String {
    match path.to_str() {
        Some(s) => s.chars().flat_map(char::escape_debug).collect(),
        None => format!("{path:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_snapshots_keep_binary_bytes_detect_edits_and_bound_reads() -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("quoted ' $() & ü");
        std::fs::write(&path, b"\0\xff\r\nlast").map_err(|e| e.to_string())?;
        let snapshot = Snapshot::read(&path, &|| true)?;
        assert_eq!(snapshot.bytes, b"\0\xff\r\nlast");
        assert!(snapshot.unchanged()?);
        std::fs::write(&path, b"new bytes").map_err(|e| e.to_string())?;
        assert!(!snapshot.unchanged()?);
        assert!(Snapshot::read(&path, &|| false).is_err());
        File::create(&path)
            .and_then(|f| f.set_len(FILE_LIMIT + 1))
            .map_err(|e| e.to_string())?;
        assert!(Snapshot::read(&path, &|| true).is_err_and(|e| e.contains("2 MiB")));
        assert!(Snapshot::read(&temp.0, &|| true).is_err());
        assert!(
            Snapshot::read(Path::new("/dev/null"), &|| true)?
                .bytes
                .is_empty()
        );
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn native_byte_paths_and_explicit_input_links_are_read_only() -> Result<(), String> {
        use std::os::unix::ffi::OsStringExt;
        let temp = crate::git::file_comparison::Scratch::new()?;
        let name = std::ffi::OsString::from_vec(b"raw\xff\nname".to_vec());
        let path = temp.0.join(name);
        std::fs::write(&path, b"original bytes").map_err(|e| e.to_string())?;
        let link = temp.0.join("input-link");
        std::os::unix::fs::symlink(&path, &link).map_err(|e| e.to_string())?;
        let snapshot = Snapshot::read(&link, &|| true)?;
        assert_eq!(snapshot.bytes, b"original bytes");
        assert!(
            label(&path).contains("\\xFF")
                || label(&path).contains("\\xFF".to_ascii_lowercase().as_str())
        );
        assert_eq!(
            std::fs::read(&path).map_err(|e| e.to_string())?,
            snapshot.bytes
        );
        assert!(Snapshot::read(Path::new("/dev/zero"), &|| true).is_err());
        Ok(())
    }
}
