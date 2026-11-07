//! Linux-local ownership: no mutable repository pathname is used for metadata writes.
use rustix::fs::{self, AtFlags, CWD, FileType, Mode, OFlags, RenameFlags};
use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn err(e: impl std::fmt::Display) -> String {
    format!("Cannot establish held object-store ownership: {e}")
}
fn flags() -> OFlags {
    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK
}
fn identity(file: &File) -> Result<(u64, u64), String> {
    let s = fs::fstat(file).map_err(err)?;
    Ok((s.st_dev, s.st_ino))
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
fn proc_path(file: &File) -> PathBuf {
    // The process id is our own, and this is a file-descriptor path, never a signal target.
    PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        file.as_raw_fd()
    ))
}
fn link_file(file: &File, directory: &File, name: &str) -> Result<(), rustix::io::Errno> {
    fs::linkat(
        CWD,
        proc_path(file),
        directory,
        name,
        AtFlags::SYMLINK_FOLLOW,
    )
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
    pub repo: Arc<File>,
    pub git: Arc<File>,
    objects: Arc<File>,
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
        if read(&alternate, 64 * 1024)? != expected {
            return Err(err("alternate content changed during capture"));
        }
        let result = Self {
            nodes,
            repo,
            git,
            objects,
            info,
            pack,
            alternate,
            expected: expected.into(),
            locked: AtomicBool::new(false),
        };
        // Proc descriptor access is required by guarded Git reads and create-only links.
        let via = File::open(proc_path(&result.repo)).map_err(err)?;
        if identity(&via)? != identity(&result.repo)? {
            return Err(err("owned descriptor access is unavailable"));
        }
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
                || (current.st_dev, current.st_ino) != identity(&node.file)?
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
    pub fn portable_identities(
        &self,
    ) -> Result<Vec<super::super::super::helper::Identity>, String> {
        let mut files = Vec::new();
        if let Some(first) = self.nodes.first() {
            files.push(first.parent.as_ref());
        }
        files.extend(self.nodes.iter().map(|node| node.file.as_ref()));
        files.push(self.alternate.as_ref());
        files
            .into_iter()
            .map(|file| {
                let (device, inode) = identity(file)?;
                let birth = file
                    .metadata()
                    .map_err(err)?
                    .created()
                    .map_err(|_| {
                        err("remote helper requires filesystem birth-time identity support")
                    })?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(err)?;
                Ok(super::super::super::helper::Identity {
                    device,
                    inode,
                    birth_seconds: birth.as_secs(),
                    birth_nanos: birth.subsec_nanos(),
                })
            })
            .collect()
    }
    pub fn run(
        &self,
        original: &Path,
        args: &[&str],
        input: Vec<u8>,
        max: usize,
        stage: Option<&Stage>,
        active: &dyn Fn() -> bool,
    ) -> Result<Vec<u8>, String> {
        let git = format!("--git-dir={}", proc_path(&self.git).display());
        let work = format!("--work-tree={}", proc_path(&self.repo).display());
        let args: Vec<_> = [git.as_str(), work.as_str()]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let object_dir = proc_path(stage.map(|s| s.root.as_ref()).unwrap_or(&self.objects));
        let env = [(
            "GIT_OBJECT_DIRECTORY",
            object_dir.to_string_lossy().into_owned(),
        )];
        let out = super::super::super::super::command::object_storage_output_with_env(
            original, &args, input, max, &env, active,
        )?;
        if out.code != 0 {
            return Err(format!(
                "Held object-store Git failed ({}): {}",
                out.code,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(out.stdout)
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
    pub fn stage(&self, name: &str) -> Result<Stage, String> {
        fs::mkdirat(self.objects.as_ref(), name, Mode::RWXU).map_err(err)?;
        let root = Arc::new(File::from(
            fs::openat(
                self.objects.as_ref(),
                name,
                flags() | OFlags::DIRECTORY,
                Mode::empty(),
            )
            .map_err(err)?,
        ));
        fs::mkdirat(root.as_ref(), "pack", Mode::RWXU).map_err(err)?;
        let pack = Arc::new(File::from(
            fs::openat(
                root.as_ref(),
                "pack",
                flags() | OFlags::DIRECTORY,
                Mode::empty(),
            )
            .map_err(err)?,
        ));
        Ok(Stage { root, pack })
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
            let recovery = link_file(&moved, &self.info, "alternates").map_err(err);
            return Err(format!(
                "Alternate identity changed during guarded rename; recovery is create-only ({recovery:?}); retained {backup}"
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
        // Create the captured bytes privately, flush, then publish by a create-only link
        // from its held inode. Neither the source nor destination resolves ancestors again.
        let name = format!("{backup}.restore");
        let mut file = File::from(
            fs::openat(
                self.info.as_ref(),
                name.as_str(),
                OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::CREATE | OFlags::EXCL,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(err)?,
        );
        file.write_all(&self.expected).map_err(err)?;
        file.sync_all().map_err(err)?;
        link_file(&file, &self.info, "alternates").map_err(|e| {
            format!(
                "Create-only restore did not replace an existing alternate: {e}; retained {name}"
            )
        })
    }
    pub fn recovery_path(&self, name: &str) -> PathBuf {
        std::fs::read_link(proc_path(&self.info))
            .unwrap_or_else(|_| proc_path(&self.info))
            .join(name)
    }
    pub fn install(&self, stage: &Stage, hash: &str) -> Result<(), String> {
        self.current(true)?;
        for suffix in ["pack", "idx"] {
            let name = format!("pack-{hash}.{suffix}");
            let source = regular(&stage.pack, &name)?;
            match link_file(&source, &self.pack, &name) {
                Ok(()) => (),
                Err(rustix::io::Errno::EXIST) => {
                    let existing = regular(&self.pack, &name)?;
                    let stat = fs::fstat(&source).map_err(err)?;
                    if stat.st_size > 128 * 1024 * 1024
                        || read(&existing, 128 * 1024 * 1024)? != read(&source, 128 * 1024 * 1024)?
                    {
                        return Err(err(
                            "an existing pack/index differs; it was not overwritten",
                        ));
                    }
                }
                Err(e) => return Err(err(e)),
            }
        }
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
pub(super) struct Stage {
    root: Arc<File>,
    pack: Arc<File>,
}
impl Stage {
    pub fn record_outcome(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(err("terminal receipt exceeds 4 MiB"));
        }
        let mut file = File::from(
            fs::openat(
                self.root.as_ref(),
                "gitcraft-outcome.pending",
                OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::CREATE | OFlags::EXCL,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(err)?,
        );
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        link_file(&file, &self.root, "gitcraft-outcome.json").map_err(err)?;
        self.root.sync_all().map_err(err)
    }
    pub fn index(&self, hash: &str) -> PathBuf {
        proc_path(&self.pack).join(format!("pack-{hash}.idx"))
    }
}
