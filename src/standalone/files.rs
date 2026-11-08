//! Bounded native-file snapshots. Display strings are never reused as filesystem paths.
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    #[cfg(unix)]
    uid: u32,
    #[cfg(unix)]
    gid: u32,
    #[cfg(target_os = "macos")]
    flags: u32,
    #[cfg(windows)]
    attributes: u32,
}
impl Stamp {
    pub fn new(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;
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
            #[cfg(unix)]
            uid: metadata.uid(),
            #[cfg(unix)]
            gid: metadata.gid(),
            #[cfg(target_os = "macos")]
            flags: std::os::macos::fs::MetadataExt::st_flags(metadata),
            #[cfg(windows)]
            attributes: metadata.file_attributes(),
        }
    }
    pub fn replacement_matches(&self, original: &Self) -> bool {
        #[cfg(target_os = "macos")]
        if self.flags != original.flags {
            return false;
        }
        #[cfg(unix)]
        {
            self.uid == original.uid
                && self.gid == original.gid
                && (self.mode & 0o7777) == (original.mode & 0o777)
        }
        #[cfg(not(unix))]
        {
            let _ = original;
            true
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Snapshot {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub stamp: Option<Stamp>,
    pub handle: Option<Arc<same_file::Handle>>,
    pub access: Option<super::access::AccessMetadata>,
}
impl Snapshot {
    pub fn read(path: &Path, active: &dyn Fn() -> bool) -> Result<Self, String> {
        if !active() {
            return Err(
                crate::i18n::text("interaction-standalone-error-comparison-cancelled").into(),
            );
        }
        if null_input(path) {
            return Ok(Self {
                path: path.into(),
                bytes: Vec::new(),
                stamp: None,
                handle: None,
                access: None,
            });
        }
        let file = open_input(path).map_err(|e| {
            crate::i18n::message(
                "interaction-standalone-error-open-file",
                &[
                    ("path", (label(path)).into()),
                    ("detail", (e.to_string()).into()),
                ],
            )
        })?;
        Self::from_file(path, file, active)
    }
    pub fn from_file(
        path: &Path,
        mut file: File,
        active: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !before.is_file() || before.len() > FILE_LIMIT {
            return Err(crate::i18n::message(
                "interaction-standalone-error-regular-file",
                &[("path", (label(path)).into())],
            ));
        }
        let stamp = Stamp::new(&before);
        let mut bytes = Vec::new();
        let mut block = [0u8; 32 * 1024];
        loop {
            if !active() {
                return Err(
                    crate::i18n::text("interaction-standalone-error-comparison-cancelled").into(),
                );
            }
            let count = file.read(&mut block).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            if bytes.len() + count > FILE_LIMIT as usize {
                return Err(
                    crate::i18n::text("interaction-standalone-error-comparison-limit").into(),
                );
            }
            bytes.extend_from_slice(&block[..count]);
        }
        if Stamp::new(&file.metadata().map_err(|e| e.to_string())?) != stamp
            || bytes.len() as u64 != stamp.len
        {
            return Err(crate::i18n::message(
                "interaction-standalone-error-file-read-changed",
                &[("path", (label(path)).into())],
            ));
        }
        Ok(Self {
            path: path.into(),
            bytes,
            stamp: Some(stamp),
            handle: Some(Arc::new(
                same_file::Handle::from_file(file).map_err(|e| e.to_string())?,
            )),
            access: None,
        })
    }
    pub fn output(path: &Path, file: File) -> Result<Self, String> {
        let before = super::access::read(&file, path)?;
        let mut snapshot = Self::from_file(path, file, &|| true)?;
        let handle = snapshot.handle.as_ref().ok_or(crate::i18n::text(
            "interaction-standalone-error-no-output-identity",
        ))?;
        let after = super::access::read(handle.as_file(), path)?;
        if before != after
            || snapshot.stamp.as_ref()
                != Some(&Stamp::new(
                    &handle.as_file().metadata().map_err(|e| e.to_string())?,
                ))
        {
            return Err(
                crate::i18n::text("interaction-standalone-error-access-read-changed").into(),
            );
        }
        snapshot.access = Some(after);
        Ok(snapshot)
    }
    pub fn unchanged(&self) -> Result<bool, String> {
        let now = Self::read(&self.path, &|| true)?;
        Ok(now.handle == self.handle && now.stamp == self.stamp && now.bytes == self.bytes)
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

pub(super) fn null_input(path: &Path) -> bool {
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
    fn native_labels_and_explicit_input_links_are_read_only() -> Result<(), String> {
        use std::os::unix::ffi::OsStringExt;
        let temp = crate::git::file_comparison::Scratch::new()?;
        let name = std::ffi::OsString::from_vec(b"raw\xff\nname".to_vec());
        let raw_path = temp.0.join(name);
        assert!(label(&raw_path).contains("\\xFF") || label(&raw_path).contains("\\xff"));
        // APFS accepts only UTF-8 filenames. Native argument/label handling is still
        // exercised above and by cli::tests, while the filesystem fixture uses a legal name.
        #[cfg(target_os = "macos")]
        let path = temp.0.join("raw\nname");
        #[cfg(not(target_os = "macos"))]
        let path = raw_path;
        std::fs::write(&path, b"original bytes").map_err(|e| e.to_string())?;
        let link = temp.0.join("input-link");
        std::os::unix::fs::symlink(&path, &link).map_err(|e| e.to_string())?;
        let snapshot = Snapshot::read(&link, &|| true)?;
        assert_eq!(snapshot.bytes, b"original bytes");
        assert_eq!(
            std::fs::read(&path).map_err(|e| e.to_string())?,
            snapshot.bytes
        );
        assert!(Snapshot::read(Path::new("/dev/zero"), &|| true).is_err());
        Ok(())
    }
}
