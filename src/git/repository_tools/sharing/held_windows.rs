//! Native Windows ownership uses kernel sharing restrictions, not pathname rechecks.
//! Every ancestor is held without FILE_SHARE_DELETE; the alternate is additionally
//! held with exclusive write/delete ownership. Detach renames the actual held file;
//! it never edits its contents, including when other clones hard-link that inode.
use std::fs::File;
use std::io::Write;
use std::os::windows::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[path = "windows_rename.rs"]
mod rename;
const SHARE_READ: u32 = 1;
const SHARE_READ_WRITE: u32 = 3;
const SHARE_ALL: u32 = 7;
const DIRECTORY: u32 = 0x0200_0000;
const NOFOLLOW: u32 = 0x0020_0000;
const REPARSE: u32 = 0x400;
fn err(e: impl std::fmt::Display) -> String {
    crate::i18n::message(
        "backend-cannot-retain-windows-object-store-ownership-e-af8dbd",
        &[("detail", (e).to_string().into())],
    )
}
fn open(path: &Path, directory: bool, write: bool) -> Result<File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(write)
        .share_mode(if write { SHARE_READ } else { SHARE_READ_WRITE })
        .custom_flags(NOFOLLOW | if directory { DIRECTORY } else { 0 })
        .open(path)
        .map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if meta.file_attributes() & REPARSE != 0
        || meta.is_dir() != directory
        || (!directory && !meta.is_file())
    {
        return Err(err(crate::i18n::text(
            "backend-reparse-points-and-non-regular-metadata-are-unsupported-43b774",
        )));
    }
    Ok(file)
}
fn locked_read(path: &Path) -> Result<File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ)
        .custom_flags(NOFOLLOW)
        .open(path)
        .map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if !meta.is_file() || meta.file_attributes() & REPARSE != 0 {
        return Err(err(crate::i18n::text(
            "backend-pack-data-must-be-a-plain-file-fa5f92",
        )));
    }
    Ok(file)
}
fn alternate(path: &Path, create: bool) -> Result<File, String> {
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE},
        Storage::FileSystem::DELETE,
    };
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(create)
        .access_mode(GENERIC_READ | DELETE | if create { GENERIC_WRITE } else { 0 })
        .share_mode(SHARE_READ)
        .create_new(create)
        .custom_flags(NOFOLLOW)
        .open(path)
        .map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if !meta.is_file() || meta.file_attributes() & REPARSE != 0 {
        return Err(err(crate::i18n::text(
            "backend-alternate-metadata-must-be-a-plain-file-93cc17",
        )));
    }
    Ok(file)
}
fn same_file(path: &Path, held: &File) -> Result<bool, String> {
    // This read must share DELETE with our retained source handle. Its requested
    // access is READ only; the held source still excludes every other writer/deleter.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE_ALL)
        .custom_flags(NOFOLLOW)
        .open(path)
        .map_err(err)?;
    let meta = file.metadata().map_err(err)?;
    if !meta.is_file() || meta.file_attributes() & REPARSE != 0 {
        return Err(err(crate::i18n::text(
            "backend-replaced-alternate-metadata-is-not-a-plain-file-12c788",
        )));
    }
    let a = winapi_util::file::information(&file).map_err(err)?;
    let b = winapi_util::file::information(held).map_err(err)?;
    Ok(a.volume_serial_number() == b.volume_serial_number() && a.file_index() == b.file_index())
}
fn read(file: &File, max: usize) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file
            .seek_read(&mut buffer, data.len() as u64)
            .map_err(err)?;
        if count == 0 {
            return Ok(data);
        }
        if data.len() + count > max {
            return Err(err(crate::i18n::text(
                "backend-metadata-exceeds-its-byte-limit-116946",
            )));
        }
        data.extend_from_slice(&buffer[..count]);
    }
}
#[derive(Debug)]
struct Node {
    file: Arc<File>,
    at: PathBuf,
    path: PathBuf,
}
#[derive(Debug)]
pub(super) struct Ownership {
    nodes: Vec<Node>,
    info: PathBuf,
    info_file: Arc<File>,
    pack: PathBuf,
    alternate: Arc<File>,
    expected: Vec<u8>,
    locked: AtomicBool,
    backup: Mutex<Option<String>>,
    installed: Mutex<Vec<Arc<File>>>,
}
impl Ownership {
    pub fn capture(
        repo: &Path,
        git: &Path,
        objects: &Path,
        expected: &[u8],
    ) -> Result<Self, String> {
        if crate::remote::is_remote(repo) {
            return Err(crate::i18n::text(
                "backend-make-independent-on-ssh-wsl-requires-an-owned-remote-helper-with-75bff7",
            )
            .into());
        }
        if !repo.is_absolute() || git != repo.join(".git") || objects != git.join("objects") {
            return Err(err(crate::i18n::text(
                "backend-a-conventional-local-git-objects-clone-is-required-dd8898",
            )));
        }
        let mut nodes = Vec::new();
        let mut path = PathBuf::new();
        for component in repo.components() {
            match component {
                Component::Prefix(prefix) => path.push(prefix.as_os_str()),
                Component::RootDir => {
                    path.push(component.as_os_str());
                    Self::hold(&mut nodes, &path)?;
                }
                Component::Normal(name) => {
                    path.push(name);
                    Self::hold(&mut nodes, &path)?;
                }
                _ => {
                    return Err(err(crate::i18n::text(
                        "backend-repository-paths-cannot-contain-or-da343b",
                    )));
                }
            }
        }
        for directory in [
            git.to_owned(),
            objects.to_owned(),
            objects.join("info"),
            objects.join("pack"),
        ] {
            Self::hold(&mut nodes, &directory)?;
        }
        let info_file = nodes
            .iter()
            .find(|node| node.path == objects.join("info"))
            .map(|node| node.file.clone())
            .ok_or_else(|| {
                err(crate::i18n::text(
                    "backend-no-retained-info-directory-bdff69",
                ))
            })?;
        let alternate = Arc::new(alternate(&objects.join("info/alternates"), false)?);
        if read(&alternate, 64 * 1024)? != expected {
            return Err(err(crate::i18n::text(
                "backend-alternate-bytes-changed-during-capture-fd9bce",
            )));
        }
        let result = Self {
            nodes,
            info: objects.join("info"),
            info_file,
            pack: objects.join("pack"),
            alternate,
            expected: expected.into(),
            locked: AtomicBool::new(false),
            backup: Mutex::new(None),
            installed: Mutex::new(Vec::new()),
        };
        result.current(true)?;
        Ok(result)
    }
    fn hold(nodes: &mut Vec<Node>, path: &Path) -> Result<(), String> {
        let file = Arc::new(open(path, true, false)?);
        let at = rename::final_path(&file).ok_or_else(|| {
            err(crate::i18n::text(
                "backend-the-system-could-not-report-the-held-directory-location-8699e1",
            ))
        })?;
        // All parent handles remain open without delete sharing before a child is opened.
        // Thus an already captured ancestor cannot be renamed/replaced between these opens.
        nodes.push(Node {
            file,
            at,
            path: path.into(),
        });
        Ok(())
    }
    pub fn current(&self, present: bool) -> Result<(), String> {
        for node in &self.nodes {
            if rename::final_path(&node.file).as_ref() != Some(&node.at)
                || node.file.metadata().map_err(err)?.file_attributes() & REPARSE != 0
            {
                return Err(err(crate::i18n::text(
                    "backend-a-held-ancestor-is-unavailable-or-changed-461243",
                )));
            }
        }
        let actual = read(&self.alternate, 64 * 1024)?;
        if actual != self.expected {
            return Err(err(crate::i18n::text(
                "backend-the-actual-held-alternate-content-changed-270b63",
            )));
        }
        let path = self.info.join("alternates");
        if present {
            if !same_file(&path, &self.alternate)? {
                return Err(err(crate::i18n::text(
                    "backend-the-alternate-name-no-longer-belongs-to-the-held-file-1970b2",
                )));
            }
        } else {
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => {
                    return Err(err(crate::i18n::text(
                        "backend-an-alternate-name-exists-after-guarded-removal-6a7d93",
                    )));
                }
            }
        }
        Ok(())
    }
    pub fn lock(&self) -> Result<Lock<'_>, String> {
        self.locked
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                err(crate::i18n::text(
                    "backend-this-independence-operation-is-already-active-15ac1c",
                ))
            })?;
        Ok(Lock(self))
    }
    pub fn detach(&self, name: &str) -> Result<(), String> {
        self.current(true)?;
        // Source and destination directory are actual retained handles. A hard-link
        // alias keeps exactly its original contents; only this directory entry moves.
        rename::no_replace(&self.alternate, &self.info_file, name).map_err(err)?;
        *self.backup.lock().map_err(|_| {
            err(crate::i18n::text(
                "backend-backup-ownership-lock-failed-5e6ba4",
            ))
        })? = Some(name.into());
        self.verify_backup(name)
    }
    pub fn verify_backup(&self, name: &str) -> Result<(), String> {
        let backup = self.backup.lock().map_err(|_| {
            err(crate::i18n::text(
                "backend-backup-ownership-lock-failed-5e6ba4",
            ))
        })?;
        let Some(held_name) = backup.as_ref() else {
            return Err(err(crate::i18n::text("backend-no-held-backup-d4b8fb")));
        };
        if held_name != name
            || !same_file(&self.info.join(name), &self.alternate)?
            || read(&self.alternate, 64 * 1024)? != self.expected
        {
            return Err(err(crate::i18n::text(
                "backend-held-backup-identity-or-captured-bytes-changed-2ce651",
            )));
        }
        Ok(())
    }
    pub fn restore(&self, name: &str) -> Result<(), String> {
        let destination = self.info.join("alternates");
        match std::fs::symlink_metadata(&destination) {
            Ok(_) => {
                if same_file(&destination, &self.alternate)?
                    && read(&self.alternate, 64 * 1024)? == self.expected
                {
                    return Ok(());
                }
                return Err(err(crate::i18n::text(
                    "backend-a-newer-alternate-file-exists-it-was-not-overwritten-1a1775",
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(err(error)),
        }
        // Publish captured bytes from a fresh actual inode, never by writing the old
        // alternate/backup. The held directory and no-replace rename guard both names.
        let private = format!("{name}.restore");
        let mut file = alternate(&self.info.join(&private), true)?;
        file.write_all(&self.expected).map_err(err)?;
        file.sync_all().map_err(err)?;
        rename::no_replace(&file, &self.info_file, "alternates").map_err(|error| {
            err(crate::i18n::message(
                "backend-create-only-restore-refused-error-retained-private-5798f2",
                &[
                    ("detail", (error).to_string().into()),
                    ("private", (private).to_string().into()),
                ],
            ))
        })?;
        if !same_file(&destination, &file)? || read(&file, 64 * 1024)? != self.expected {
            return Err(err(crate::i18n::text(
                "backend-restored-alternate-identity-content-could-not-be-proven-b3d3a6",
            )));
        }
        Ok(())
    }
    pub fn install(
        &self,
        hash: &str,
        pack: &[u8],
        index: &[u8],
        nonce: &str,
    ) -> Result<(), String> {
        self.current(true)?;
        let directory = open(&self.pack, true, false)?;
        let mut installed = self.installed.lock().map_err(|_| {
            err(crate::i18n::text(
                "backend-installed-pack-lock-failed-64eb56",
            ))
        })?;
        for (suffix, bytes) in [("pack", pack), ("idx", index)] {
            let name = format!("pack-{hash}.{suffix}");
            let private = format!(".gitcraft-{nonce}.{suffix}");
            let mut source = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(SHARE_READ)
                .access_mode(0x80000000 | 0x40000000 | 0x00010000)
                .custom_flags(NOFOLLOW)
                .open(self.pack.join(&private))
                .map_err(err)?;
            let write = source.write_all(bytes).and_then(|_| source.sync_all());
            if let Err(e) = write {
                let cleanup = rename::discard(&source);
                return Err(err(crate::i18n::message(
                    "backend-e-owned-temporary-cleanup-cleanup-retained-if-cleanup-failed-6f4f0d",
                    &[
                        ("detail", (e).to_string().into()),
                        ("cleanup", format!("{:?}", cleanup).into()),
                        (
                            "path",
                            (self.pack.join(&private).display()).to_string().into(),
                        ),
                    ],
                )));
            }
            match rename::no_replace(&source, &directory, &name) {
                Ok(()) => {
                    installed.push(Arc::new(source));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let existing = match locked_read(&self.pack.join(&name)) {
                        Ok(existing) => existing,
                        Err(e) => {
                            let cleanup = rename::discard(&source);
                            return Err(err(crate::i18n::message(
                                "backend-e-owned-temporary-cleanup-cleanup-retained-if-cleanup-failed-6f4f0d",
                                &[
                                    ("detail", (e).to_string().into()),
                                    ("cleanup", format!("{:?}", cleanup).into()),
                                    (
                                        "path",
                                        (self.pack.join(&private).display()).to_string().into(),
                                    ),
                                ],
                            )));
                        }
                    };
                    let comparison = read(&existing, 128 << 20);
                    let cleanup = rename::discard(&source);
                    if comparison.as_ref().is_err()
                        || comparison.as_ref().is_ok_and(|v| v != bytes)
                        || cleanup.is_err()
                    {
                        return Err(err(crate::i18n::message(
                            "backend-existing-pack-differs-or-cannot-be-verified-was-not-overwritten-o-d6cb93",
                            &[
                                ("cleanup", format!("{:?}", cleanup).into()),
                                (
                                    "path",
                                    (self.pack.join(&private).display()).to_string().into(),
                                ),
                            ],
                        )));
                    }
                    installed.push(Arc::new(existing));
                }
                Err(e) => {
                    let cleanup = rename::discard(&source);
                    return Err(err(crate::i18n::message(
                        "backend-e-owned-temporary-cleanup-cleanup-retained-if-cleanup-failed-6f4f0d",
                        &[
                            ("detail", (e).to_string().into()),
                            ("cleanup", format!("{:?}", cleanup).into()),
                            (
                                "path",
                                (self.pack.join(&private).display()).to_string().into(),
                            ),
                        ],
                    )));
                }
            }
        }
        self.current(true)
    }
}
pub(super) struct Lock<'a>(&'a Ownership);
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        self.0.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod backend_fluent_ownership_diagnostics {
    #[test]
    fn backend_fluent_held_windows_refusal_and_opaque_details_without_io() {
        use std::path::Path;
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let detail = "OS error 5: C:\\raw\\de\\repo";
            let error = super::err(detail);
            assert!(error.ends_with(detail));
            assert!(error.starts_with(if language == "de" {
                "Die Kontrolle über den Windows-Objektspeicher"
            } else {
                "Cannot retain Windows object-store ownership"
            }));
            let refusal = super::Ownership::capture(
                Path::new("relative"),
                Path::new("other"),
                Path::new("objects"),
                b"opaque",
            )
            .err();
            assert!(refusal.is_some_and(|e| e.contains(if language == "de" {
                "Ein herkömmlicher lokaler Klon"
            } else {
                "a conventional local .git/objects clone"
            })));
        }
        crate::i18n::set_language("en-US");
    }
}
