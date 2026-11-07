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
    format!("Cannot retain Windows object-store ownership: {e}")
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
        return Err(err(
            "reparse points and non-regular metadata are unsupported",
        ));
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
        return Err(err("pack data must be a plain file"));
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
        return Err(err("alternate metadata must be a plain file"));
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
        return Err(err("replaced alternate metadata is not a plain file"));
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
            return Err(err("metadata exceeds its byte limit"));
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
    repo: PathBuf,
    git: PathBuf,
    objects: PathBuf,
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
            return Err("Make independent on SSH/WSL requires an owned remote helper with held filesystem identities. Read-only inspection remains available.".into());
        }
        if !repo.is_absolute() || git != repo.join(".git") || objects != git.join("objects") {
            return Err(err("a conventional local .git/objects clone is required"));
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
                _ => return Err(err("repository paths cannot contain . or ..")),
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
            .ok_or_else(|| err("no retained info directory"))?;
        let alternate = Arc::new(alternate(&objects.join("info/alternates"), false)?);
        if read(&alternate, 64 * 1024)? != expected {
            return Err(err("alternate bytes changed during capture"));
        }
        let result = Self {
            nodes,
            repo: repo.into(),
            git: git.into(),
            objects: objects.into(),
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
        let at = crate::safe_read::final_path(&file)
            .ok_or_else(|| err("the system could not report the held directory location"))?;
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
            if crate::safe_read::final_path(&node.file).as_ref() != Some(&node.at)
                || node.file.metadata().map_err(err)?.file_attributes() & REPARSE != 0
            {
                return Err(err("a held ancestor is unavailable or changed"));
            }
        }
        let actual = read(&self.alternate, 64 * 1024)?;
        if actual != self.expected {
            return Err(err("the actual held alternate content changed"));
        }
        let path = self.info.join("alternates");
        if present {
            if !same_file(&path, &self.alternate)? {
                return Err(err("the alternate name no longer belongs to the held file"));
            }
        } else {
            match std::fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                _ => return Err(err("an alternate name exists after guarded removal")),
            }
        }
        Ok(())
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
        let git = format!("--git-dir={}", self.git.display());
        let work = format!("--work-tree={}", self.repo.display());
        let args: Vec<_> = [git.as_str(), work.as_str()]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let env = [(
            "GIT_OBJECT_DIRECTORY",
            stage
                .map(|s| &s.root)
                .unwrap_or(&self.objects)
                .to_string_lossy()
                .into_owned(),
        )];
        let out = super::super::super::super::command::object_storage_output_with_env(
            original, &args, input, max, &env, active,
        )?;
        if out.code != 0 {
            return Err(format!(
                "Held Windows Git failed ({}): {}",
                out.code,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(out.stdout)
    }
    pub fn lock(&self) -> Result<Lock<'_>, String> {
        self.locked
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| err("this independence operation is already active"))?;
        Ok(Lock(self))
    }
    pub fn stage(&self, name: &str) -> Result<Stage, String> {
        self.current(true)?;
        let root = self.objects.join(name);
        std::fs::create_dir(&root).map_err(err)?;
        let root_file = Arc::new(open(&root, true, false)?);
        let pack = root.join("pack");
        std::fs::create_dir(&pack).map_err(err)?;
        let pack_file = Arc::new(open(&pack, true, false)?);
        Ok(Stage {
            root,
            pack,
            _root_file: root_file,
            _pack_file: pack_file,
        })
    }
    pub fn detach(&self, name: &str) -> Result<(), String> {
        self.current(true)?;
        // Source and destination directory are actual retained handles. A hard-link
        // alias keeps exactly its original contents; only this directory entry moves.
        rename::no_replace(&self.alternate, &self.info_file, name).map_err(err)?;
        *self
            .backup
            .lock()
            .map_err(|_| err("backup ownership lock failed"))? = Some(name.into());
        self.verify_backup(name)
    }
    pub fn verify_backup(&self, name: &str) -> Result<(), String> {
        let backup = self
            .backup
            .lock()
            .map_err(|_| err("backup ownership lock failed"))?;
        let Some(held_name) = backup.as_ref() else {
            return Err(err("no held backup"));
        };
        if held_name != name
            || !same_file(&self.info.join(name), &self.alternate)?
            || read(&self.alternate, 64 * 1024)? != self.expected
        {
            return Err(err("held backup identity or captured bytes changed"));
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
                return Err(err("a newer alternate file exists; it was not overwritten"));
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
            err(format!(
                "create-only restore refused: {error}; retained {private}"
            ))
        })?;
        if !same_file(&destination, &file)? || read(&file, 64 * 1024)? != self.expected {
            return Err(err(
                "restored alternate identity/content could not be proven",
            ));
        }
        Ok(())
    }
    pub fn recovery_path(&self, name: &str) -> PathBuf {
        self.info.join(name)
    }
    pub fn install(&self, stage: &Stage, hash: &str) -> Result<(), String> {
        self.current(true)?;
        let mut installed = self
            .installed
            .lock()
            .map_err(|_| err("installed pack ownership lock failed"))?;
        for suffix in ["pack", "idx"] {
            let name = format!("pack-{hash}.{suffix}");
            let source = locked_read(&stage.pack.join(&name))?;
            if source.metadata().map_err(err)?.len() > 128 * 1024 * 1024 {
                return Err(err("staged pack/index exceeds 128 MiB"));
            }
            let destination = self.pack.join(&name);
            let retained = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(SHARE_READ)
                .custom_flags(NOFOLLOW)
                .open(&destination)
            {
                Ok(mut file) => {
                    let mut offset = 0;
                    let mut buffer = [0u8; 64 * 1024];
                    loop {
                        let count = source.seek_read(&mut buffer, offset).map_err(err)?;
                        if count == 0 {
                            break;
                        }
                        file.write_all(&buffer[..count]).map_err(|error|format!("Owned pack copy failed; borrowing remains and the partial copy was retained: {error}"))?;
                        offset += count as u64;
                    }
                    file.sync_all().map_err(err)?;
                    file
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let existing = locked_read(&destination)?;
                    if read(&existing, 128 * 1024 * 1024)? != read(&source, 128 * 1024 * 1024)? {
                        return Err(err(
                            "an existing pack/index differs and was not overwritten",
                        ));
                    }
                    existing
                }
                Err(error) => return Err(err(error)),
            };
            // Keep the actual installed files protected through proof/recovery as well.
            installed.push(Arc::new(retained));
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
pub(super) struct Stage {
    root: PathBuf,
    pack: PathBuf,
    _root_file: Arc<File>,
    _pack_file: Arc<File>,
}
impl Stage {
    pub fn record_outcome(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(err("terminal receipt exceeds 4 MiB"));
        }
        let mut file = alternate(&self.root.join("gitcraft-outcome.pending"), true)?;
        file.write_all(bytes).map_err(err)?;
        file.sync_all().map_err(err)?;
        rename::no_replace(&file, &self._root_file, "gitcraft-outcome.json").map_err(err)
    }
    pub fn index(&self, hash: &str) -> PathBuf {
        self.pack.join(format!("pack-{hash}.idx"))
    }
}
