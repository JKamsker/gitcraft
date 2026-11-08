//! Unix ownership, including macOS: no mutable repository pathname is used for metadata writes.
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, RenameFlags};
use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn err(e: impl std::fmt::Display) -> String {
    format!("Cannot establish held object-store ownership: {e}")
}
fn flags() -> OFlags {
    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK
}
fn identity(file: &File) -> Result<(u64, u64), String> {
    let s = file.metadata().map_err(err)?;
    Ok((s.dev(), s.ino()))
}
fn regular(dir: &File, name: &str) -> Result<File, String> {
    let file = File::from(fs::openat(dir, name, flags(), Mode::empty()).map_err(err)?);
    let stat = fs::fstat(&file).map_err(err)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
        return Err(err("metadata is not a regular file"));
    }
    Ok(file)
}
fn read(file: &File, max: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read_at(&mut buffer, out.len() as u64).map_err(err)?;
        if count == 0 {
            break;
        }
        if out.len() + count > max {
            return Err(err("metadata exceeds its byte limit"));
        }
        out.extend_from_slice(&buffer[..count]);
    }
    Ok(out)
}
#[derive(Debug)]
struct Node {
    parent: Arc<File>,
    name: OsString,
    file: Arc<File>,
}
#[derive(Debug)]
pub(super) struct Ownership {
    nodes: Vec<Node>,
    info: Arc<File>,
    pack: Arc<File>,
    alternate: Arc<File>,
    expected: Vec<u8>,
    locked: AtomicBool,
}
impl Ownership {
    pub fn capture(
        repo: &Path,
        git: &Path,
        objects: &Path,
        expected: &[u8],
    ) -> Result<Self, String> {
        if crate::remote::is_remote(repo) {
            return Err("Make independent is unavailable over SSH/WSL: this transport cannot retain filesystem ownership handles. Read-only inspection remains available.".into());
        }
        if !repo.is_absolute() || git != repo.join(".git") || objects != git.join("objects") {
            return Err("Make independent requires a local conventional clone with its own .git/objects directory.".into());
        }
        let root = Arc::new(File::from(
            fs::open("/", flags() | OFlags::DIRECTORY, Mode::empty()).map_err(err)?,
        ));
        let mut nodes = Vec::new();
        let mut parent = root;
        for component in repo.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => {
                    parent = Self::child(&mut nodes, &parent, name)?;
                }
                _ => {
                    return Err(err(
                        "repository paths must be absolute without . or .. components",
                    ));
                }
            }
        }
        let repo = parent;
        let git = Self::child(&mut nodes, &repo, std::ffi::OsStr::new(".git"))?;
        let objects = Self::child(&mut nodes, &git, std::ffi::OsStr::new("objects"))?;
        let info = Self::child(&mut nodes, &objects, std::ffi::OsStr::new("info"))?;
        let pack = Self::child(&mut nodes, &objects, std::ffi::OsStr::new("pack"))?;
        let alternate = Arc::new(regular(&info, "alternates")?);
        if alternate.metadata().map_err(err)?.nlink() != 1 {
            return Err(err("hard-linked alternate metadata is unsupported"));
        }
        if read(&alternate, 64 * 1024)? != expected {
            return Err(err("alternate content changed during capture"));
        }
        let result = Self {
            nodes,
            info,
            pack,
            alternate,
            expected: expected.into(),
            locked: AtomicBool::new(false),
        };
        result.current(true)?;
        Ok(result)
    }
    fn child(
        nodes: &mut Vec<Node>,
        parent: &Arc<File>,
        name: &std::ffi::OsStr,
    ) -> Result<Arc<File>, String> {
        let file = Arc::new(File::from(
            fs::openat(
                parent.as_ref(),
                name,
                flags() | OFlags::DIRECTORY,
                Mode::empty(),
            )
            .map_err(err)?,
        ));
        nodes.push(Node {
            parent: parent.clone(),
            name: name.into(),
            file: file.clone(),
        });
        Ok(file)
    }
    pub fn current(&self, alternate_present: bool) -> Result<(), String> {
        for node in &self.nodes {
            let current = fs::statat(
                node.parent.as_ref(),
                node.name.as_os_str(),
                AtFlags::SYMLINK_NOFOLLOW,
            )
            .map_err(err)?;
            if FileType::from_raw_mode(current.st_mode) != FileType::Directory
                || identity(&File::from(
                    fs::openat(
                        node.parent.as_ref(),
                        node.name.as_os_str(),
                        flags() | OFlags::DIRECTORY,
                        Mode::empty(),
                    )
                    .map_err(err)?,
                ))? != identity(&node.file)?
            {
                return Err(err("repository or metadata ancestor identity changed"));
            }
        }
        if alternate_present {
            let current = regular(&self.info, "alternates")?;
            if identity(&current)? != identity(&self.alternate)?
                || read(&current, 64 * 1024)? != self.expected
            {
                return Err(err("alternate file identity or content changed"));
            }
        } else {
            match fs::statat(self.info.as_ref(), "alternates", AtFlags::SYMLINK_NOFOLLOW) {
                Err(rustix::io::Errno::NOENT) => (),
                Ok(_) => return Err("An alternate link was created during final proof; independence was not committed".into()),
                Err(error) => return Err(err(error)),
            }
        }
        Ok(())
    }
    pub fn lock(&self) -> Result<Lock<'_>, String> {
        self.locked
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "This prepared independence operation is already running".to_owned())?;
        if let Err(error) = fs::flock(
            self.alternate.as_ref(),
            fs::FlockOperation::NonBlockingLockExclusive,
        ) {
            self.locked.store(false, Ordering::Release);
            return Err(format!(
                "Could not lock the inspected alternate file: {error}"
            ));
        }
        Ok(Lock(self))
    }
    pub fn detach(&self, backup: &str) -> Result<(), String> {
        self.current(true)?;
        // Atomic create-only rename retains the actual removed inode as recovery evidence.
        // There is no unlink of the mutable alternate pathname.
        fs::renameat_with(
            self.info.as_ref(),
            "alternates",
            self.info.as_ref(),
            backup,
            RenameFlags::NOREPLACE,
        )
        .map_err(err)?;
        let moved = regular(&self.info, backup)?;
        if identity(&moved)? != identity(&self.alternate)? {
            let recovery = fs::renameat_with(
                self.info.as_ref(),
                backup,
                self.info.as_ref(),
                "alternates",
                RenameFlags::NOREPLACE,
            );
            return Err(format!(
                "Alternate identity changed during guarded rename; recovery: {recovery:?}; retained {backup}"
            ));
        }
        Ok(())
    }
    pub fn verify_backup(&self, name: &str) -> Result<(), String> {
        let backup = regular(&self.info, name)?;
        if identity(&backup)? != identity(&self.alternate)?
            || read(&backup, 64 * 1024)? != self.expected
        {
            return Err("Retained alternate backup identity or content changed".into());
        }
        Ok(())
    }
    pub fn restore(&self, backup: &str) -> Result<(), String> {
        if let Ok(existing) = regular(&self.info, "alternates") {
            if identity(&existing)? == identity(&self.alternate)?
                && read(&existing, 64 * 1024)? == self.expected
            {
                return Ok(());
            }
            return Err("A newer alternate file exists; it was not overwritten.".into());
        }
        fs::renameat_with(
            self.info.as_ref(),
            backup,
            self.info.as_ref(),
            "alternates",
            RenameFlags::NOREPLACE,
        )
        .map_err(err)
    }
    pub fn install(
        &self,
        hash: &str,
        pack: &[u8],
        index: &[u8],
        nonce: &str,
    ) -> Result<(), String> {
        self.current(true)?;
        for (suffix, bytes) in [("pack", pack), ("idx", index)] {
            let name = format!("pack-{hash}.{suffix}");
            let private = format!(".gitcraft-{nonce}.{suffix}");
            let mut source = File::from(
                fs::openat(
                    self.pack.as_ref(),
                    private.as_str(),
                    OFlags::RDWR
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::RUSR | Mode::WUSR,
                )
                .map_err(err)?,
            );
            source.write_all(bytes).map_err(err)?;
            source.sync_all().map_err(err)?;
            match fs::renameat_with(
                self.pack.as_ref(),
                private.as_str(),
                self.pack.as_ref(),
                name.as_str(),
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => (),
                Err(rustix::io::Errno::EXIST) => {
                    let existing = regular(&self.pack, &name)?;
                    if read(&existing, 128 << 20)? != bytes {
                        return Err(err("an existing pack differs and was not overwritten"));
                    }
                    fs::unlinkat(self.pack.as_ref(), private.as_str(), AtFlags::empty())
                        .map_err(err)?;
                }
                Err(e) => return Err(err(e)),
            }
        }
        self.pack.sync_all().map_err(err)?;
        self.current(true)
    }
}
pub(super) struct Lock<'a>(&'a Ownership);
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let _ = fs::flock(self.0.alternate.as_ref(), fs::FlockOperation::Unlock);
        self.0.locked.store(false, Ordering::Release);
    }
}
