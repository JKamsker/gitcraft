//! Retained kernel claim/fence handles and immutable namespace identities.
//! Callers must keep the fence in the actual mutation/dispatch owner, never just its client.
use crate::{PrivateDirectory, PrivateFileMode, file_identity};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
const NAMES: [&str; 3] = ["primary", "fence", "generation"];
const VERSION: &str = "gitcraft-workspace-anchor-v1";

#[derive(Clone, Debug)]
pub struct WorkspaceGrant {
    pub root: PathBuf,
    pub anchor: String,
    pub generation: String,
}
pub struct WorkspaceClaim {
    coordination: Coordination,
    generation: String,
}
pub struct WorkspaceFence {
    coordination: Coordination,
    generation: String,
}
struct Coordination {
    path: PathBuf,
    directory: PrivateDirectory,
    files: [File; 3],
    anchor: File,
    anchor_identity: String,
    identities: Vec<String>,
    bytes: Vec<u8>,
}
fn hex_generation(generation: &str) -> bool {
    generation.len() == 64
        && generation
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn anchor_path(root: &Path) -> io::Result<PathBuf> {
    let name = root
        .file_name()
        .ok_or_else(|| io::Error::other("coordination root has no name"))?;
    let mut anchor = name.to_os_string();
    anchor.push(".anchor");
    Ok(root.with_file_name(anchor))
}
fn open_anchor(root: &Path, create: bool) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(create).create_new(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(3).custom_flags(0x00200000);
        if create {
            use windows::Win32::Storage::FileSystem::{
                FILE_GENERIC_READ, FILE_GENERIC_WRITE, WRITE_DAC, WRITE_OWNER,
            };
            options
                .access_mode((FILE_GENERIC_READ | FILE_GENERIC_WRITE | WRITE_DAC | WRITE_OWNER).0);
        }
    }
    let file = options.open(anchor_path(root)?)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > 4096 {
        return Err(io::Error::other("invalid workspace anchor file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: credential query only.
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 || meta.nlink() != 1
        {
            return Err(io::Error::other(
                "workspace anchor is not private single-link file",
            ));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("workspace anchor is a reparse point"));
        }
        if !create {
            crate::receipt_retirement::private_acl(&file)?;
        }
    }
    Ok(file)
}
fn read_file(file: &File, max: u64) -> io::Result<Vec<u8>> {
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(io::Error::other("oversized coordination file"));
    }
    Ok(bytes)
}
fn lock(file: &File) -> io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}
fn lock_until(file: &File, waiting: &mut impl FnMut() -> bool) -> io::Result<()> {
    loop {
        if !waiting() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "workspace fence expired or cancelled",
            ));
        }
        if lock(file)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(5));
    }
}
impl Coordination {
    fn identities(directory: &PrivateDirectory, files: &[File; 3]) -> io::Result<Vec<String>> {
        let mut ids = vec![directory.identity()?];
        for file in files {
            ids.push(file_identity(file)?);
        }
        Ok(ids)
    }
    fn bytes(identities: &[String]) -> Vec<u8> {
        format!("{VERSION}\n{}\n", identities.join("\n")).into_bytes()
    }
    fn existing(path: &Path) -> io::Result<Self> {
        let anchor = open_anchor(path, false)?;
        let anchor_identity = file_identity(&anchor)?;
        let bytes = read_file(&anchor, 4096)?;
        let directory = PrivateDirectory::open(path)?;
        let files = [
            directory.open_file(NAMES[0], PrivateFileMode::Update)?,
            directory.open_file(NAMES[1], PrivateFileMode::Update)?,
            directory.open_file(NAMES[2], PrivateFileMode::Update)?,
        ];
        let identities = Self::identities(&directory, &files)?;
        if bytes != Self::bytes(&identities) {
            return Err(io::Error::other(
                "workspace lock namespace replaced or anchor incomplete; recovery preserved",
            ));
        }
        let result = Self {
            path: path.to_path_buf(),
            directory,
            files,
            anchor,
            anchor_identity,
            identities,
            bytes,
        };
        result.verify_names()?;
        Ok(result)
    }
    /// Only a newly exclusive-created directory may initialize. An existing namespace
    /// with a missing anchor is never repaired/replaced automatically.
    fn initialize(path: &Path) -> io::Result<Self> {
        crate::create_private_directory(path)?;
        let directory = PrivateDirectory::open(path)?;
        let primary = directory.open_file(NAMES[0], PrivateFileMode::CreateNew)?;
        if !lock(&primary)? {
            return Err(io::Error::other(
                "workspace primary initialization unavailable",
            ));
        }
        let fence = directory.open_file(NAMES[1], PrivateFileMode::CreateNew)?;
        let generation = directory.open_file(NAMES[2], PrivateFileMode::CreateNew)?;
        let files = [primary, fence, generation];
        let identities = Self::identities(&directory, &files)?;
        let bytes = Self::bytes(&identities);
        let mut anchor = open_anchor(path, true)?;
        #[cfg(windows)]
        protect_anchor(&anchor, &directory)?;
        anchor.write_all(&bytes)?;
        anchor.sync_all()?;
        let anchor_identity = file_identity(&anchor)?;
        let result = Self {
            path: path.to_path_buf(),
            directory,
            files,
            anchor,
            anchor_identity,
            identities,
            bytes,
        };
        result.verify_names()?;
        Ok(result)
    }
    fn verify_names(&self) -> io::Result<()> {
        let named = PrivateDirectory::open(&self.path)?;
        let mut actual = vec![named.identity()?];
        for name in NAMES {
            actual.push(file_identity(
                &self.directory.open_file(name, PrivateFileMode::Read)?,
            )?);
        }
        validate_namespace(&self.identities, &actual)?;
        let anchor = open_anchor(&self.path, false)?;
        if file_identity(&anchor)? != self.anchor_identity
            || read_file(&anchor, 4096)? != self.bytes
            || read_file(&self.anchor, 4096)? != self.bytes
        {
            return Err(io::Error::other("workspace anchor changed"));
        }
        Ok(())
    }
}
impl WorkspaceClaim {
    /// `root` is one canonical actual-machine repository coordination path, outside the
    /// removable workspace. `waiting` is the caller's monotonic budget/cancellation policy.
    pub fn acquire(
        root: &Path,
        generation: &str,
        mut waiting: impl FnMut() -> bool,
    ) -> io::Result<Self> {
        if !hex_generation(generation) {
            return Err(io::Error::other("invalid workspace generation"));
        }
        if !waiting() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "workspace claim cancelled",
            ));
        }
        let (mut coordination, initialized) = match open_anchor(root, false) {
            Ok(_) => (Coordination::existing(root)?, false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (Coordination::initialize(root)?, true)
            }
            Err(error) => return Err(error),
        };
        if !initialized && !lock(&coordination.files[0])? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "another workspace claimant owns this repository",
            ));
        }
        lock_until(&coordination.files[1], &mut waiting)?;
        coordination.verify_names()?;
        if !waiting() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "workspace claim cancelled before publication",
            ));
        }
        let file = &mut coordination.files[2];
        file.seek(SeekFrom::Start(0))?;
        file.set_len(0)?;
        file.write_all(format!("{generation}\n").as_bytes())?;
        file.sync_all()?;
        // Unlock only our retained operation-fence file; primary stays locked until Drop.
        coordination.files[1].unlock()?;
        Ok(Self {
            coordination,
            generation: generation.to_owned(),
        })
    }
    pub fn grant(&self) -> WorkspaceGrant {
        WorkspaceGrant {
            root: self.coordination.path.clone(),
            anchor: self.coordination.anchor_identity.clone(),
            generation: self.generation.clone(),
        }
    }
}
impl WorkspaceFence {
    /// Must execute in the actual durable owner. Dropping this returns only its retained
    /// fence handle; it never clears, dispatches, retires or changes any generation.
    pub fn acquire(grant: &WorkspaceGrant, mut waiting: impl FnMut() -> bool) -> io::Result<Self> {
        if !hex_generation(&grant.generation) {
            return Err(io::Error::other("invalid workspace generation"));
        }
        let coordination = Coordination::existing(&grant.root)?;
        lock_until(&coordination.files[1], &mut waiting)?;
        coordination.verify_names()?;
        validate_grant(
            &grant.anchor,
            &grant.generation,
            &coordination.anchor_identity,
            &read_file(&coordination.files[2], 128)?,
        )?;
        // The old generation is not authority after its real claim helper exits. Any
        // error other than WouldBlock remains Unknown, never evidence of a live claimant.
        if lock(&coordination.files[0])? {
            return Err(io::Error::other(
                "workspace primary owner is no longer live",
            ));
        }
        Ok(Self {
            coordination,
            generation: grant.generation.clone(),
        })
    }
    pub fn publish_record(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > 2048
            || !name.starts_with(&format!("{}-", self.generation))
            || !name.ends_with(".owner")
        {
            return Err(io::Error::other("invalid bounded generation owner record"));
        }
        self.coordination.verify_names()?;
        let mut file = self
            .coordination
            .directory
            .open_file(name, PrivateFileMode::CreateNew)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        crate::close_file(file)
    }
    pub fn verify(&self, grant: &WorkspaceGrant) -> io::Result<()> {
        self.coordination.verify_names()?;
        validate_grant(
            &grant.anchor,
            &grant.generation,
            &self.coordination.anchor_identity,
            &read_file(&self.coordination.files[2], 128)?,
        )
    }
}
fn validate_namespace(expected: &[String], actual: &[String]) -> io::Result<()> {
    if expected.len() != 4 || actual != expected {
        return Err(io::Error::other(
            "workspace directory or lock file replaced/unlinked; no authority",
        ));
    }
    Ok(())
}
fn validate_grant(
    expected_anchor: &str,
    expected_generation: &str,
    actual_anchor: &str,
    actual_generation: &[u8],
) -> io::Result<()> {
    if expected_anchor != actual_anchor
        || actual_generation != format!("{expected_generation}\n").as_bytes()
    {
        return Err(io::Error::other(
            "stale workspace generation or anchor; no mutation authorized",
        ));
    }
    Ok(())
}
#[cfg(windows)]
fn protect_anchor(file: &File, directory: &PrivateDirectory) -> io::Result<()> {
    directory.protect_file(file)
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn replacement_and_missing_lock_never_become_a_second_namespace() {
        let original: Vec<String> = ["directory", "primary", "fence", "generation"]
            .map(str::to_owned)
            .to_vec();
        let mut mutations = 0;
        for index in 0..original.len() {
            let mut replaced = original.clone();
            replaced[index] = "replacement".into();
            if validate_namespace(&original, &replaced).is_ok() {
                mutations += 1;
            }
            let mut missing = original.clone();
            missing.remove(index);
            if validate_namespace(&original, &missing).is_ok() {
                mutations += 1;
            }
        }
        assert_eq!(mutations, 0);
        assert!(validate_namespace(&original, &original).is_ok());
    }
    #[test]
    fn queued_generation_and_replaced_anchor_have_no_mutation_authority() {
        let old = "a".repeat(64);
        let new = "b".repeat(64);
        let mut mutations = 0;
        for (anchor, generation) in [
            ("original", format!("{new}\n")),
            ("replacement", format!("{old}\n")),
            ("original", "partial".into()),
        ] {
            if validate_grant("original", &old, anchor, generation.as_bytes()).is_ok() {
                mutations += 1;
            }
        }
        assert_eq!(mutations, 0);
        assert!(
            validate_grant("original", &new, "original", format!("{new}\n").as_bytes()).is_ok()
        );
        for invalid in ["", "PID 424242", "stale timestamp"] {
            assert!(!hex_generation(invalid));
        }
    }
}
