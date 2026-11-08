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
    crate::i18n::message(
        "backend-cannot-establish-held-object-store-ownership-e-52cc72",
        &[("detail", (e).to_string().into())],
    )
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
        return Err(err(crate::i18n::text(
            "backend-metadata-is-not-a-regular-file-680efc",
        )));
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
            return Err(err(crate::i18n::text(
                "backend-metadata-exceeds-its-byte-limit-116946",
            )));
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
            return Err(crate::i18n::text(
                "backend-make-independent-is-unavailable-over-ssh-wsl-this-transport-canno-95fb58",
            )
            .into());
        }
        if !repo.is_absolute() || git != repo.join(".git") || objects != git.join("objects") {
            return Err(crate::i18n::text(
                "backend-make-independent-requires-a-local-conventional-clone-with-its-own-f9f879",
            )
            .into());
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
                    return Err(err(crate::i18n::text(
                        "backend-repository-paths-must-be-absolute-without-or-components-5976bf",
                    )));
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
            return Err(err(crate::i18n::text(
                "backend-hard-linked-alternate-metadata-is-unsupported-e3c0aa",
            )));
        }
        if read(&alternate, 64 * 1024)? != expected {
            return Err(err(crate::i18n::text(
                "backend-alternate-content-changed-during-capture-33483d",
            )));
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
                return Err(err(crate::i18n::text(
                    "backend-repository-or-metadata-ancestor-identity-changed-84cc47",
                )));
            }
        }
        if alternate_present {
            let current = regular(&self.info, "alternates")?;
            if identity(&current)? != identity(&self.alternate)?
                || read(&current, 64 * 1024)? != self.expected
            {
                return Err(err(crate::i18n::text(
                    "backend-alternate-file-identity-or-content-changed-56b09f",
                )));
            }
        } else {
            match fs::statat(self.info.as_ref(), "alternates", AtFlags::SYMLINK_NOFOLLOW) {
                Err(rustix::io::Errno::NOENT) => (),
                Ok(_) => return Err(crate::i18n::text("backend-an-alternate-link-was-created-during-final-proof-independence-was-81494f").into()),
                Err(error) => return Err(err(error)),
            }
        }
        Ok(())
    }
    pub fn lock(&self) -> Result<Lock<'_>, String> {
        self.locked
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                crate::i18n::text(
                    "backend-this-prepared-independence-operation-is-already-running-09db5e",
                )
                .to_owned()
            })?;
        if let Err(error) = fs::flock(
            self.alternate.as_ref(),
            fs::FlockOperation::NonBlockingLockExclusive,
        ) {
            self.locked.store(false, Ordering::Release);
            return Err(crate::i18n::message(
                "backend-could-not-lock-the-inspected-alternate-file-error-2bc960",
                &[("detail", (error).to_string().into())],
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
            return Err(crate::i18n::message(
                "backend-alternate-identity-changed-during-guarded-rename-recovery-recover-7673aa",
                &[
                    ("recovery", format!("{:?}", recovery).into()),
                    ("backup", (backup).to_string().into()),
                ],
            ));
        }
        Ok(())
    }
    pub fn verify_backup(&self, name: &str) -> Result<(), String> {
        let backup = regular(&self.info, name)?;
        if identity(&backup)? != identity(&self.alternate)?
            || read(&backup, 64 * 1024)? != self.expected
        {
            return Err(crate::i18n::text(
                "backend-retained-alternate-backup-identity-or-content-changed-8be403",
            )
            .into());
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
            return Err(crate::i18n::text(
                "backend-a-newer-alternate-file-exists-it-was-not-overwritten-7f799b",
            )
            .into());
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
        // Private entries are relative to a retained 0700 staging-directory handle.
        // Cleanup never resolves a replaceable .gitcraft-* pathname in objects/pack.
        let staging = format!(".gitcraft-{nonce}");
        fs::mkdirat(
            self.pack.as_ref(),
            staging.as_str(),
            Mode::RUSR | Mode::WUSR | Mode::XUSR,
        )
        .map_err(err)?;
        let stage = PackStage {
            directory: File::from(
                fs::openat(
                    self.pack.as_ref(),
                    staging.as_str(),
                    flags() | OFlags::DIRECTORY,
                    Mode::empty(),
                )
                .map_err(err)?,
            ),
        };
        let result = (|| {
            for (suffix, bytes) in [("pack", pack), ("idx", index)] {
                let name = format!("pack-{hash}.{suffix}");
                let mut source = File::from(
                    fs::openat(
                        &stage.directory,
                        suffix,
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
                    &stage.directory,
                    suffix,
                    self.pack.as_ref(),
                    name.as_str(),
                    RenameFlags::NOREPLACE,
                ) {
                    Ok(()) => {
                        let installed = regular(&self.pack, &name)?;
                        if identity(&installed)? != identity(&source)? {
                            return Err(err(crate::i18n::text(
                                "backend-installed-pack-identity-changed-8739f6",
                            )));
                        }
                    }
                    Err(rustix::io::Errno::EXIST) => {
                        let existing = regular(&self.pack, &name)?;
                        if read(&existing, 128 << 20)? != bytes {
                            return Err(err(crate::i18n::text(
                                "backend-an-existing-pack-differs-and-was-not-overwritten-fdbff8",
                            )));
                        }
                    }
                    Err(e) => return Err(err(e)),
                }
            }
            self.pack.sync_all().map_err(err)?;
            self.current(true)
        })();
        let cleanup = stage.cleanup();
        match (result, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (result, cleanup) => Err(pack_installation_error(result, cleanup, &staging)),
        }
    }
}
fn pack_installation_error(
    result: Result<(), String>,
    cleanup: Result<(), String>,
    staging: &str,
) -> String {
    crate::i18n::message(
        "backend-pack-installation-staging-cleanup-cleanup-retained-private-direct-480fa1",
        &[
            (
                "result",
                result
                    .err()
                    .unwrap_or_else(|| crate::i18n::text("backend-completed-4ddb3e").into())
                    .into(),
            ),
            ("cleanup", format!("{cleanup:?}").into()),
            ("staging", staging.into()),
        ],
    )
}

pub(super) struct Lock<'a>(&'a Ownership);
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let _ = fs::flock(self.0.alternate.as_ref(), fs::FlockOperation::Unlock);
        self.0.locked.store(false, Ordering::Release);
    }
}

struct PackStage {
    directory: File,
}
fn retire_pack_stage(mut remove: impl FnMut(&str) -> Result<(), String>) -> Result<(), String> {
    let mut errors = Vec::new();
    for name in ["pack", "idx"] {
        if let Err(e) = remove(name) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
impl PackStage {
    fn cleanup(&self) -> Result<(), String> {
        retire_pack_stage(
            |name| match fs::unlinkat(&self.directory, name, AtFlags::empty()) {
                Ok(()) | Err(rustix::io::Errno::NOENT) => Ok(()),
                Err(e) => Err(e.to_string()),
            },
        )
    }
}
impl Drop for PackStage {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
#[cfg(test)]
mod pack_cleanup_recording {
    use super::*;
    #[test]
    fn staging_cleanup_is_relative_fixed_and_attempts_both_files_after_failure() {
        let mut names = Vec::new();
        let result = retire_pack_stage(|name| {
            names.push(name.to_owned());
            if name == "pack" {
                Err("recorded failure".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(names, ["pack", "idx"]);
        assert!(result.err().is_some_and(|e| e.contains("recorded failure")));
    }
}

#[cfg(test)]
mod backend_fluent_ownership_diagnostics {
    #[test]
    fn backend_fluent_held_unix_refusal_and_details_are_presented_without_io() {
        use std::path::Path;
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            // Supplied nonconventional path returns before the first filesystem open.
            let refusal = super::Ownership::capture(
                Path::new("relative/repo"),
                Path::new("relative/repo/.git"),
                Path::new("relative/repo/.git/objects"),
                b"opaque donor bytes",
            )
            .err();
            assert_eq!(
                refusal.as_deref(),
                Some(if language == "de" {
                    "Das Unabhängigmachen erfordert einen herkömmlichen lokalen Klon mit einem eigenen Verzeichnis .git/objects."
                } else {
                    "Make independent requires a local conventional clone with its own .git/objects directory."
                })
            );
            let detail = "OS error 13: /raw/de/repo/.git/objects";
            assert_eq!(
                super::err(detail),
                if language == "de" {
                    format!(
                        "Die gehaltene Kontrolle über den Objektspeicher konnte nicht hergestellt werden: {detail}"
                    )
                } else {
                    format!("Cannot establish held object-store ownership: {detail}")
                }
            );
            let envelope = super::pack_installation_error(
                Err(detail.into()),
                Err("opaque cleanup failure /raw/pack".into()),
                ".gitcraft-raw-nonce",
            );
            assert!(envelope.contains(detail));
            assert!(envelope.contains("Err(\"opaque cleanup failure /raw/pack\")"));
            assert!(envelope.contains(".git/objects/pack/.gitcraft-raw-nonce"));
            let completed = super::pack_installation_error(
                Ok(()),
                Err("opaque cleanup".into()),
                ".gitcraft-retained",
            );
            assert!(completed.contains(if language == "de" {
                "abgeschlossen"
            } else {
                "completed"
            }));
        }
        crate::i18n::set_language("en-US");
    }
}
