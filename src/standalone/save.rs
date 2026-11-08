//! Preserve the displaced inode, then install create-only. No blind destination overwrite.
use super::directory::Directory;
use super::files::{FILE_LIMIT, Snapshot, label};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug)]
pub(super) struct Output {
    pub path: PathBuf,
    requested_parent: PathBuf,
    parent: Arc<Directory>,
    name: OsString,
    original: Option<Snapshot>,
}
#[derive(Debug)]
pub(super) struct Saved {
    pub previous: Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    BeforeMove,
    AfterMove,
    BeforeInstall,
    AfterInstall,
    BeforeUnlock,
    AfterLockCapture,
}

impl Output {
    pub fn initial_bytes(&self) -> Option<&[u8]> {
        self.original.as_ref().map(|s| s.bytes.as_slice())
    }
    pub fn capture(path: &Path, inputs: &[Snapshot]) -> Result<Self, String> {
        if super::files::null_input(path) {
            return Err(
                crate::i18n::text("interaction-standalone-error-output-empty-device").into(),
            );
        }
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .ok_or(crate::i18n::text(
                "interaction-standalone-error-output-filename",
            ))?
            .to_owned();
        let requested_parent = path
            .parent()
            .ok_or(crate::i18n::text(
                "interaction-standalone-error-no-result-parent",
            ))?
            .to_owned();
        let parent = std::fs::canonicalize(&requested_parent).map_err(|e| {
            crate::i18n::message(
                "interaction-standalone-error-open-parent",
                &[("detail", (e.to_string()).into())],
            )
        })?;
        let parent = Arc::new(Directory::open(&parent).map_err(|e| e.to_string())?);
        let mut output = Self {
            path,
            requested_parent,
            parent,
            name,
            original: None,
        };
        output.original = output.current()?;
        if let Some(original) = &output.original {
            if inputs
                .iter()
                .any(|i| i.handle.is_some() && i.handle == original.handle)
            {
                return Err(crate::i18n::text("interaction-standalone-error-output-alias").into());
            }
            if original
                .handle
                .as_ref()
                .ok_or(crate::i18n::text(
                    "interaction-standalone-error-no-output-identity",
                ))?
                .as_file()
                .metadata()
                .map_err(|e| e.to_string())?
                .permissions()
                .readonly()
            {
                return Err(
                    crate::i18n::text("interaction-standalone-error-output-readonly").into(),
                );
            }
        }
        Ok(output)
    }
    fn current(&self) -> Result<Option<Snapshot>, String> {
        match self.parent.read(&self.name) {
            Ok(file) => {
                drop(file);
                self.parent.snapshot(&self.name, &self.path).map(Some)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(crate::i18n::message(
                "interaction-standalone-error-output-plain-file",
                &[("detail", (e.to_string()).into())],
            )),
        }
    }
    fn guard(&self, inputs: &[Snapshot]) -> Result<(), String> {
        if !self.parent_current() {
            return Err(crate::i18n::text("interaction-standalone-error-parent-identity").into());
        }
        if !same_optional(self.current()?.as_ref(), self.original.as_ref()) {
            return Err(crate::i18n::text("interaction-standalone-error-merged-changed").into());
        }
        for input in inputs {
            if !input.unchanged()? {
                return Err(crate::i18n::message(
                    "interaction-standalone-error-input-changed",
                    &[("path", (label(&input.path)).into())],
                ));
            }
        }
        Ok(())
    }
    fn parent_current(&self) -> bool {
        self.parent.unchanged()
            && std::fs::canonicalize(&self.requested_parent)
                .is_ok_and(|now| now == self.parent.path)
    }
    pub fn save(&self, bytes: &[u8], inputs: &[Snapshot]) -> Result<Saved, String> {
        self.save_with(bytes, inputs, &mut |_| {})
    }
    fn save_with(
        &self,
        bytes: &[u8],
        inputs: &[Snapshot],
        hook: &mut dyn FnMut(Phase),
    ) -> Result<Saved, String> {
        if bytes.len() as u64 > FILE_LIMIT {
            return Err(crate::i18n::text("interaction-standalone-error-output-limit").into());
        }
        self.guard(inputs)?;
        let mut lock = Lock::new(Arc::clone(&self.parent), &self.name)?;
        // Every ordinary return after reservation passes through explicit release,
        // including preparation/guard errors. Drop is only an unwind fallback.
        let result = (|| {
            self.guard(inputs)?;
            let mut recovery = Recovery::new(Arc::clone(&self.parent))?;
            let candidate = OsStr::new("replacement");
            let previous = OsStr::new("previous");
            let file = super::permissions::candidate(
                recovery.dir()?,
                &self.parent,
                &self.name,
                self.original.as_ref(),
                bytes,
            )?;
            drop(file);
            let prepared = recovery
                .dir()?
                .snapshot(candidate, &recovery.dir()?.path.join(candidate))?;
            if let Some(original) = &self.original
                && (!prepared
                    .access
                    .as_ref()
                    .zip(original.access.as_ref())
                    .is_some_and(|(a, b)| a.replacement_matches(b))
                    || !prepared
                        .stamp
                        .as_ref()
                        .zip(original.stamp.as_ref())
                        .is_some_and(|(a, b)| a.replacement_matches(b)))
            {
                return Err(
                    crate::i18n::text("interaction-standalone-error-prepared-policy").into(),
                );
            }
            // Probe before moving MERGED. Unsupported hard-link filesystems fail with
            // the original destination untouched; there is deliberately no copy fallback.
            recovery
                .dir()?
                .link(candidate, recovery.dir()?, OsStr::new("probe"))
                .map_err(|e| {
                    crate::i18n::message(
                        "interaction-standalone-error-atomic-install",
                        &[("detail", (e.to_string()).into())],
                    )
                })?;
            recovery
                .dir()?
                .remove_file(OsStr::new("probe"))
                .map_err(|e| e.to_string())?;
            self.guard(inputs)?;
            hook(Phase::BeforeMove);
            let backup = if self.original.is_some() {
                self.parent
                    .move_file(&self.name, recovery.dir()?, previous)
                    .map_err(|e| {
                        crate::i18n::message(
                            "interaction-standalone-error-previous-result",
                            &[("detail", (e.to_string()).into())],
                        )
                    })?;
                recovery.keep = true;
                hook(Phase::AfterMove);
                let displaced = recovery.dir()?.snapshot(
                    previous,
                    &recovery.parent.path.join(&recovery.name).join(previous),
                );
                let displaced = match displaced {
                    Ok(snapshot) => snapshot,
                    Err(e) => {
                        recovery.restore(self);
                        return Err(recovery.error(&e));
                    }
                };
                if !same_optional(Some(&displaced), self.original.as_ref())
                    || !self.parent_current()
                {
                    // Restore only into absence. A concurrent creator keeps its own file.
                    recovery.restore(self);
                    return Err(recovery.error(crate::i18n::text(
                        "interaction-standalone-error-save-identity",
                    )));
                }
                Some(recovery.dir()?.path.join(previous))
            } else {
                None
            };
            hook(Phase::BeforeInstall);
            if !self.parent_current()
                || inputs.iter().any(|i| i.unchanged() != Ok(true))
                || !self.displaced_current(&recovery)
            {
                if backup.is_some() {
                    recovery.restore(self);
                }
                return Err(recovery.error(crate::i18n::text(
                    "interaction-standalone-error-before-install",
                )));
            }
            if let Err(e) = recovery.dir()?.link(candidate, &self.parent, &self.name) {
                if backup.is_some() {
                    recovery.restore(self);
                }
                return Err(recovery.error(&crate::i18n::message(
                    "interaction-standalone-error-no-overwrite",
                    &[("detail", (e.to_string()).into())],
                )));
            }
            hook(Phase::AfterInstall);
            let installed = self.current().map_err(|e| recovery.error(&e))?;
            if !same_optional(installed.as_ref(), Some(&prepared))
                || !self.parent_current()
                || inputs.iter().any(|i| i.unchanged() != Ok(true))
                || !self.displaced_current(&recovery)
            {
                return Err(recovery.error(crate::i18n::text(
                    "interaction-standalone-error-during-save",
                )));
            }
            Ok(Saved { previous: backup })
        })();
        lock.finish(result, hook)
    }
    fn displaced_current(&self, recovery: &Recovery) -> bool {
        if self.original.is_none() {
            return true;
        }
        recovery
            .dir()
            .and_then(|dir| dir.snapshot(OsStr::new("previous"), &dir.path.join("previous")))
            .is_ok_and(|now| same_optional(Some(&now), self.original.as_ref()))
    }
}
fn same_optional(a: Option<&Snapshot>, b: Option<&Snapshot>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.handle == b.handle && a.stamp == b.stamp && a.bytes == b.bytes && a.access == b.access
        }
        _ => false,
    }
}

struct Lock {
    parent: Arc<Directory>,
    recovery: Recovery,
    name: OsString,
    handle: Option<same_file::Handle>,
    active: bool,
}
impl Lock {
    fn new(parent: Arc<Directory>, target: &OsStr) -> Result<Self, String> {
        let name = OsString::from(format!(
            ".gitcraft-merge-{:x}.lock",
            md5::compute(target.as_encoded_bytes())
        ));
        // Establish identity in our private sibling before publishing a reservation.
        // Failure before the create-only link cannot leave an unowned public lock.
        let mut recovery = Recovery::new(Arc::clone(&parent))?;
        recovery.keep = true;
        let file = recovery
            .dir()?
            .create(OsStr::new("reservation"), &[])
            .map_err(|e| {
                recovery.error(&crate::i18n::message(
                    "interaction-standalone-error-create-reservation",
                    &[("detail", (e.to_string()).into())],
                ))
            })?;
        let handle =
            same_file::Handle::from_file(file).map_err(|e| recovery.error(&e.to_string()))?;
        recovery
            .dir()?
            .link(OsStr::new("reservation"), &parent, &name)
            .map_err(|e| {
                recovery.error(&crate::i18n::message(
                    "interaction-standalone-error-reserve-result",
                    &[("detail", (e.to_string()).into())],
                ))
            })?;
        Ok(Self {
            parent,
            recovery,
            name,
            handle: Some(handle),
            active: true,
        })
    }
    fn finish<T>(
        &mut self,
        result: Result<T, String>,
        hook: &mut dyn FnMut(Phase),
    ) -> Result<T, String> {
        match (result, self.release_with(hook)) {
            (result, Ok(())) => result,
            (Ok(_), Err(release)) => Err(crate::i18n::message(
                "interaction-standalone-error-save-maybe-installed",
                &[("detail", (release).into())],
            )),
            (Err(error), Err(release)) => Err(crate::i18n::message(
                "interaction-standalone-error-save-and-release",
                &[("error", (error).into()), ("release", (release).into())],
            )),
        }
    }
    fn release_with(&mut self, hook: &mut dyn FnMut(Phase)) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        // Never retry an uncertain capture from Drop, where it could move a new owner.
        self.active = false;
        let result: Result<(), String> = (|| {
            let owned = self.parent.unchanged()
                && self
                    .parent
                    .read(&self.name)
                    .and_then(same_file::Handle::from_file)
                    .is_ok_and(|h| self.handle.as_ref() == Some(&h));
            if !owned {
                return Err(crate::i18n::text("interaction-standalone-error-lock-changed").into());
            }
            hook(Phase::BeforeUnlock); // Capture any late replacement; never unlink it.
            let captured = OsStr::new("released-lock");
            let dir = self.recovery.dir()?;
            self.parent
                .move_file(&self.name, dir, captured)
                .map_err(|e| e.to_string())?;
            hook(Phase::AfterLockCapture);
            let owned = dir
                .read(captured)
                .and_then(same_file::Handle::from_file)
                .is_ok_and(|h| self.handle.as_ref() == Some(&h));
            if !owned {
                // POSIX linkat without SYMLINK_FOLLOW preserves the symlink itself.
                // Windows cannot promise that, so an uninspectable entry stays here.
                #[cfg(unix)]
                let can_restore = true;
                #[cfg(not(unix))]
                let can_restore = dir
                    .read(captured)
                    .and_then(|f| f.metadata())
                    .is_ok_and(|m| m.is_file());
                if can_restore {
                    let _ = dir.link(captured, &self.parent, &self.name);
                }
                return Err(crate::i18n::text("interaction-standalone-error-foreign-lock").into());
            }
            if !self.parent.unchanged() {
                return Err(crate::i18n::text("interaction-standalone-error-lock-parent").into());
            }
            // Only private, inspected entries are removed. The mutable public name
            // is never unlinked, even when another creator installs a new lock.
            drop(self.handle.take());
            dir.remove_file(captured).map_err(|e| e.to_string())?;
            dir.remove_file(OsStr::new("reservation"))
                .map_err(|e| e.to_string())?;
            drop(self.recovery.directory.take());
            self.parent
                .remove_child(&self.recovery.name)
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        result.map_err(|e| {
            self.recovery.error(&crate::i18n::message(
                "interaction-standalone-error-lock-release",
                &[("detail", (e.to_string()).into())],
            ))
        })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.release_with(&mut |_| {});
    }
}
struct Recovery {
    parent: Arc<Directory>,
    name: OsString,
    directory: Option<Directory>,
    keep: bool,
}
impl Recovery {
    fn new(parent: Arc<Directory>) -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let name = OsString::from(format!(
                ".gitcraft-merge-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            match parent.child(&name) {
                Ok(directory) => {
                    return Ok(Self {
                        parent,
                        name,
                        directory: Some(directory),
                        keep: false,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(crate::i18n::text("interaction-standalone-error-recovery-reserve").into())
    }
    fn dir(&self) -> Result<&Directory, String> {
        self.directory
            .as_ref()
            .ok_or_else(|| crate::i18n::text("interaction-standalone-error-recovery-closed").into())
    }
    fn error(&self, message: &str) -> String {
        crate::i18n::message(
            "interaction-standalone-error-recovery-location",
            &[
                ("detail", (message).into()),
                ("path", (label(&self.parent.path.join(&self.name))).into()),
            ],
        )
    }
    fn restore(&self, output: &Output) {
        let Some(dir) = &self.directory else {
            return;
        };
        #[cfg(not(unix))]
        if !dir
            .read(OsStr::new("previous"))
            .and_then(|f| f.metadata())
            .is_ok_and(|m| m.is_file())
        {
            // Windows hard_link may follow a link: retain the link itself for
            // recovery rather than creating an alias to its outside target.
            return;
        }
        let _ = dir.link(OsStr::new("previous"), &output.parent, &output.name);
    }
}
impl Drop for Recovery {
    fn drop(&mut self) {
        if let Some(dir) = &self.directory {
            let _ = dir.remove_file(OsStr::new("replacement"));
            let _ = dir.remove_file(OsStr::new("probe"));
        }
        // Windows must release our deny-delete directory handle before remove_dir.
        drop(self.directory.take());
        if !self.keep {
            let _ = self.parent.remove_child(&self.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
    fn read(path: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    // These reservation tests use only owned filesystem entries: no Git, process,
    // platform metadata helper, credential flow or transport is invoked.
    #[cfg(unix)]
    #[test]
    fn lock_release_preserves_regular_and_symlink_replacements_on_all_returns() -> Result<(), String>
    {
        for is_symlink in [false, true] {
            for operation_ok in [false, true] {
                let temp = crate::git::file_comparison::Scratch::new()?;
                let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
                let mut lock = Lock::new(parent, OsStr::new("merged"))?;
                let public = temp.0.join(&lock.name);
                let displaced = temp.0.join("owned-moved-aside");
                let outside = temp.0.join("outside");
                write(&outside, b"OUTSIDE")?;
                let recovery = lock.recovery.dir()?.path.clone();
                let mut hook_error = None;
                let operation = if operation_ok {
                    Ok(())
                } else {
                    Err("original preparation failure".into())
                };
                let result = lock.finish(operation, &mut |at| {
                    if at != Phase::BeforeUnlock {
                        return;
                    }
                    let action = std::fs::rename(&public, &displaced).and_then(|_| {
                        if is_symlink {
                            std::os::unix::fs::symlink(&outside, &public)
                        } else {
                            std::fs::write(&public, b"FOREIGN-LOCK-BYTES")
                        }
                    });
                    if let Err(e) = action {
                        hook_error = Some(e.to_string());
                    }
                });
                if let Some(error) = hook_error {
                    return Err(error);
                }
                let error = result
                    .err()
                    .ok_or("Foreign replacement reported clean success")?;
                assert!(error.contains("Save lock release uncertain"));
                assert!(error.contains(&recovery.to_string_lossy().to_string()));
                assert_eq!(
                    error.contains("original preparation failure"),
                    !operation_ok
                );
                drop(lock); // No second attempt is allowed to remove the restored owner.
                if is_symlink {
                    assert_eq!(
                        std::fs::read_link(&public).map_err(|e| e.to_string())?,
                        outside
                    );
                    assert_eq!(
                        std::fs::read_link(recovery.join("released-lock"))
                            .map_err(|e| e.to_string())?,
                        outside
                    );
                } else {
                    assert_eq!(read(&public)?, b"FOREIGN-LOCK-BYTES");
                    assert_eq!(
                        read(&recovery.join("released-lock"))?,
                        b"FOREIGN-LOCK-BYTES"
                    );
                }
                assert_eq!(read(&outside)?, b"OUTSIDE");
                assert_eq!(read(&displaced)?, b"");
            }
        }
        Ok(())
    }
    #[test]
    fn lock_release_retains_captured_foreign_entry_when_new_owner_occupies_public_name()
    -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
        let mut lock = Lock::new(parent, OsStr::new("merged"))?;
        let public = temp.0.join(&lock.name);
        let recovery = lock.recovery.dir()?.path.clone();
        let mut error = None;
        let result = lock.finish(Ok(()), &mut |at| {
            let action = match at {
                Phase::BeforeUnlock => std::fs::rename(&public, temp.0.join("owned"))
                    .and_then(|_| std::fs::write(&public, b"CAPTURED-FOREIGN")),
                Phase::AfterLockCapture => std::fs::write(&public, b"NEW-OWNER"),
                _ => Ok(()),
            };
            if let Err(e) = action {
                error = Some(e.to_string());
            }
        });
        if let Some(error) = error {
            return Err(error);
        }
        assert!(result.is_err());
        drop(lock);
        assert_eq!(read(&public)?, b"NEW-OWNER");
        assert_eq!(read(&recovery.join("released-lock"))?, b"CAPTURED-FOREIGN");
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn lock_release_uncertainty_reaches_actual_save_success_and_error_returns() -> Result<(), String>
    {
        for operation_ok in [false, true] {
            let temp = crate::git::file_comparison::Scratch::new()?;
            let merged = temp.0.join("merged");
            let output = Output::capture(&merged, &[])?;
            let public = temp.0.join(format!(
                ".gitcraft-merge-{:x}.lock",
                md5::compute(b"merged")
            ));
            let mut fixture_error = None;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                let action = if at == Phase::BeforeInstall && !operation_ok {
                    std::fs::write(&merged, b"CONCURRENT-OUTPUT")
                } else if at == Phase::BeforeUnlock {
                    std::fs::rename(&public, temp.0.join("owned"))
                        .and_then(|_| std::fs::write(&public, b"FOREIGN"))
                } else {
                    Ok(())
                };
                if let Err(e) = action {
                    fixture_error = Some(e.to_string());
                }
            });
            if let Some(error) = fixture_error {
                return Err(error);
            }
            let error = result.err().ok_or("Save caller missed lock uncertainty")?;
            assert!(error.contains("Save lock release uncertain"));
            assert_eq!(
                error.contains("Could not install without overwrite"),
                !operation_ok
            );
            assert_eq!(read(&public)?, b"FOREIGN");
            assert_eq!(
                read(&merged)?,
                if operation_ok {
                    b"candidate".as_slice()
                } else {
                    b"CONCURRENT-OUTPUT".as_slice()
                }
            );
        }
        Ok(())
    }
    #[test]
    fn lock_release_reports_missing_reservation_and_preserves_original_error() -> Result<(), String>
    {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
        let mut lock = Lock::new(parent, OsStr::new("merged"))?;
        std::fs::rename(temp.0.join(&lock.name), temp.0.join("moved"))
            .map_err(|e| e.to_string())?;
        let error = lock
            .finish::<()>(Err("preparation refused".into()), &mut |_| {})
            .err()
            .ok_or("Missing reservation was ignored")?;
        assert!(error.contains("preparation refused") && error.contains("release uncertain"));
        Ok(())
    }
    #[test]
    fn lock_release_normal_and_drop_paths_remove_only_owned_reservations() -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let parent = Arc::new(Directory::open(&temp.0).map_err(|e| e.to_string())?);
        for explicit in [false, true] {
            let mut lock = Lock::new(Arc::clone(&parent), OsStr::new("merged"))?;
            let public = temp.0.join(&lock.name);
            let recovery = lock.recovery.dir()?.path.clone();
            if explicit {
                lock.finish(Ok(()), &mut |_| {})?;
            }
            drop(lock);
            assert!(!public.exists() && !recovery.exists());
        }
        let lock = Lock::new(parent, OsStr::new("merged"))?;
        let public = temp.0.join(&lock.name);
        std::fs::rename(&public, temp.0.join("owned"))
            .and_then(|_| std::fs::write(&public, b"FOREIGN"))
            .map_err(|e| e.to_string())?;
        drop(lock);
        assert_eq!(read(&public)?, b"FOREIGN");
        Ok(())
    }
    #[test]
    fn explicit_save_retains_the_old_inode_and_never_writes_inputs() -> Result<(), String> {
        use std::io::{Seek, SeekFrom, Write};
        let temp = crate::git::file_comparison::Scratch::new()?;
        let source = temp.0.join("local");
        let merged = temp.0.join("merged");
        write(&source, b"input\0\xff")?;
        write(&merged, b"original")?;
        let inputs = vec![Snapshot::read(&source, &|| true)?];
        let output = Output::capture(&merged, &inputs)?;
        let mut writer = None;
        let saved = output.save_with(b"new result", &inputs, &mut |phase| {
            if phase == Phase::BeforeMove {
                writer = std::fs::OpenOptions::new().write(true).open(&merged).ok();
            }
        })?;
        assert_eq!(read(&merged)?, b"new result");
        assert_eq!(read(&source)?, b"input\0\xff");
        let backup = saved.previous.ok_or("No displaced recovery file")?;
        assert_eq!(read(&backup)?, b"original");
        let mut writer = writer.ok_or("No fixture writer")?;
        writer
            .seek(SeekFrom::Start(0))
            .and_then(|_| writer.write_all(b"late edit"))
            .map_err(|e| e.to_string())?;
        assert_eq!(read(&backup)?, b"late edit");
        assert_eq!(read(&merged)?, b"new result");
        Ok(())
    }
    #[test]
    fn destination_drift_and_concurrent_creators_are_preserved_without_success()
    -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        for (phase, expected) in [
            (Phase::BeforeMove, b"changed before move".as_slice()),
            (Phase::BeforeInstall, b"concurrent creator".as_slice()),
            (Phase::AfterInstall, b"concurrent result edit".as_slice()),
        ] {
            let path = temp.0.join(format!("{phase:?}"));
            write(&path, b"original")?;
            let output = Output::capture(&path, &[])?;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                if at == phase {
                    let _ = std::fs::write(&path, expected);
                }
            });
            assert!(result.is_err());
            assert_eq!(read(&path)?, expected);
        }
        let path = temp.0.join("changed");
        write(&path, b"original")?;
        let output = Output::capture(&path, &[])?;
        write(&path, b"new owner bytes")?;
        assert!(output.save(b"candidate", &[]).is_err());
        assert_eq!(read(&path)?, b"new owner bytes");
        Ok(())
    }
    #[test]
    fn missing_outputs_install_create_only_aliases_and_read_only_outputs_are_refused()
    -> Result<(), String> {
        assert!(Output::capture(Path::new("/dev/null"), &[]).is_err());
        #[cfg(windows)]
        assert!(Output::capture(Path::new("NUL"), &[]).is_err());
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("new");
        let output = Output::capture(&path, &[])?;
        assert!(output.save(b"new bytes", &[])?.previous.is_none());
        assert_eq!(read(&path)?, b"new bytes");
        let input = Snapshot::read(&path, &|| true)?;
        assert!(Output::capture(&path, &[input]).is_err());
        let hard = temp.0.join("hard-alias");
        std::fs::hard_link(&path, &hard).map_err(|e| e.to_string())?;
        assert!(Output::capture(&hard, &[Snapshot::read(&path, &|| true)?]).is_err());
        let original_permissions = std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&path, readonly).map_err(|e| e.to_string())?;
        assert!(Output::capture(&path, &[]).is_err());
        std::fs::set_permissions(&path, original_permissions).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn swapped_parents_and_output_links_never_redirect_writes() -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let a = temp.0.join("a");
        let b = temp.0.join("b");
        std::fs::create_dir(&a)
            .and_then(|_| std::fs::create_dir(&b))
            .map_err(|e| e.to_string())?;
        write(&a.join("result"), b"a original")?;
        write(&b.join("result"), b"b sentinel")?;
        let alias = temp.0.join("parent-link");
        std::os::unix::fs::symlink(&a, &alias).map_err(|e| e.to_string())?;
        let output = Output::capture(&alias.join("result"), &[])?;
        std::fs::remove_file(&alias).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&b, &alias).map_err(|e| e.to_string())?;
        assert!(output.save(b"candidate", &[]).is_err());
        assert_eq!(read(&a.join("result"))?, b"a original");
        assert_eq!(read(&b.join("result"))?, b"b sentinel");
        let linked = temp.0.join("output-link");
        std::os::unix::fs::symlink(b.join("result"), &linked).map_err(|e| e.to_string())?;
        assert!(Output::capture(&linked, &[]).is_err());
        assert_eq!(read(&b.join("result"))?, b"b sentinel");
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn acl_only_tightening_during_each_save_boundary_refuses_stale_policy_and_retains_recovery()
    -> Result<(), String> {
        use rustix::fs::{XattrFlags, fgetxattr, fsetxattr};
        use std::os::unix::fs::MetadataExt;
        fn acl(uid: u32, permission: u16) -> Vec<u8> {
            let mut bytes = 2u32.to_le_bytes().to_vec();
            for (tag, perm, id) in [
                (1u16, 6u16, u32::MAX),
                (2, permission, uid),
                (4, 4, u32::MAX),
                (16, 4, u32::MAX),
                (32, 0, u32::MAX),
            ] {
                bytes.extend_from_slice(&tag.to_le_bytes());
                bytes.extend_from_slice(&perm.to_le_bytes());
                bytes.extend_from_slice(&id.to_le_bytes());
            }
            bytes
        }
        let temp = crate::git::file_comparison::Scratch::new()?;
        for phase in [
            Phase::BeforeMove,
            Phase::AfterMove,
            Phase::BeforeInstall,
            Phase::AfterInstall,
        ] {
            let path = temp.0.join(format!("acl-{phase:?}"));
            write(&path, b"original")?;
            let source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let uid = source
                .metadata()
                .map_err(|e| e.to_string())?
                .uid()
                .checked_add(1)
                .ok_or("No fixture uid")?;
            let before = acl(uid, 4);
            let tightened = acl(uid, 0);
            fsetxattr(
                &source,
                "system.posix_acl_access",
                &before,
                XattrFlags::empty(),
            )
            .map_err(|e| e.to_string())?;
            let output = Output::capture(&path, &[])?;
            let original_stamp = output.original.as_ref().ok_or("No output")?.stamp.clone();
            let mut hook_error = None;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                if at == phase
                    && let Err(e) = fsetxattr(
                        &source,
                        "system.posix_acl_access",
                        &tightened,
                        XattrFlags::empty(),
                    )
                {
                    hook_error = Some(e.to_string());
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(
                result.is_err(),
                "{phase:?} must not report a successful stale-policy save"
            );
            assert_eq!(
                Some(super::super::files::Stamp::new(
                    &source.metadata().map_err(|e| e.to_string())?
                )),
                original_stamp,
                "ACL-only edit must leave the old guarded fields identical"
            );
            let mut bytes = [0; 128];
            let n = fgetxattr(&source, "system.posix_acl_access", &mut bytes[..])
                .map_err(|e| e.to_string())?;
            assert_eq!(&bytes[..n], tightened);
            let recoveries = std::fs::read_dir(&temp.0)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("previous").is_file())
                .collect::<Vec<_>>();
            assert!(
                !recoveries.is_empty(),
                "displaced bytes and tightened metadata must remain recoverable"
            );
            if phase != Phase::AfterInstall {
                assert_eq!(read(&path)?, b"original");
                let now = Output::capture(&path, &[])?;
                assert_ne!(
                    now.original.as_ref().ok_or("No restored output")?.access,
                    output.original.as_ref().ok_or("No source")?.access
                );
            }
        }
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn xattr_only_drift_since_open_or_after_copy_is_not_a_successful_save() -> Result<(), String> {
        use rustix::fs::{XattrFlags, fsetxattr};
        let temp = crate::git::file_comparison::Scratch::new()?;
        for late in [false, true] {
            let path = temp.0.join(if late { "after-copy" } else { "since-open" });
            write(&path, b"original")?;
            let source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            fsetxattr(
                &source,
                "user.gitcraft-policy",
                b"before",
                XattrFlags::empty(),
            )
            .map_err(|e| e.to_string())?;
            let output = Output::capture(&path, &[])?;
            if !late {
                fsetxattr(
                    &source,
                    "user.gitcraft-policy",
                    b"after",
                    XattrFlags::empty(),
                )
                .map_err(|e| e.to_string())?;
            }
            let mut hook_error = None;
            let result = output.save_with(b"candidate", &[], &mut |phase| {
                if late
                    && phase == Phase::BeforeMove
                    && let Err(e) = fsetxattr(
                        &source,
                        "user.gitcraft-policy",
                        b"after",
                        XattrFlags::empty(),
                    )
                {
                    hook_error = Some(e.to_string());
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(result.is_err());
            assert_eq!(read(&path)?, b"original");
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_mac_acl_clone_preserves_policy_and_acl_only_drift_refuses_success()
    -> Result<(), String> {
        use std::os::unix::fs::MetadataExt;
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("native-acl");
        write(&path, b"original")?;
        let uid = std::fs::metadata(&path)
            .map_err(|e| e.to_string())?
            .uid()
            .to_string();
        let initial = vec![exacl::AclEntry::allow_user(&uid, exacl::Perm::READ, None)];
        exacl::setfacl(&[&path], &initial, None).map_err(|e| e.to_string())?;
        let expected =
            exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
        let saved = Output::capture(&path, &[])?.save(b"new result", &[])?;
        assert_eq!(read(&path)?, b"new result");
        assert_eq!(
            exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?,
            expected
        );
        assert_eq!(
            read(&saved.previous.ok_or("No retained original")?)?,
            b"original"
        );
        for phase in [
            Phase::BeforeMove,
            Phase::AfterMove,
            Phase::BeforeInstall,
            Phase::AfterInstall,
        ] {
            write(&path, b"original")?;
            exacl::setfacl(&[&path], &initial, None).map_err(|e| e.to_string())?;
            let output = Output::capture(&path, &[])?;
            let source_metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            let changed = vec![exacl::AclEntry::allow_user(
                &uid,
                exacl::Perm::empty(),
                None,
            )];
            let mut hook_error = None;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                if at == phase {
                    let target = if matches!(phase, Phase::BeforeMove) {
                        Ok(path.clone())
                    } else {
                        std::fs::read_dir(&temp.0)
                            .map_err(|e| e.to_string())
                            .and_then(|entries| {
                                entries
                                    .filter_map(Result::ok)
                                    .map(|e| e.path().join("previous"))
                                    .find(|p| {
                                        std::fs::metadata(p).is_ok_and(|m| {
                                            m.dev() == source_metadata.dev()
                                                && m.ino() == source_metadata.ino()
                                        })
                                    })
                                    .ok_or("No displaced source".into())
                            })
                    };
                    match target.and_then(|target| {
                        exacl::setfacl(&[target], &changed, None).map_err(|e| e.to_string())
                    }) {
                        Ok(()) => {}
                        Err(e) => hook_error = Some(e),
                    }
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(
                result.is_err(),
                "{phase:?} must reject descriptor-only drift"
            );
            if phase != Phase::AfterInstall {
                assert_eq!(read(&path)?, b"original");
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_dacl_only_tightening_at_save_boundaries_never_reports_success()
    -> Result<(), String> {
        let program = PathBuf::from(std::env::var_os("SystemRoot").ok_or("No SystemRoot")?)
            .join("System32/icacls.exe");
        if !program.is_absolute() {
            return Err("Fixture requires absolute SystemRoot".into());
        }
        let change = |path: &Path, grant: bool| -> Result<(), String> {
            let command =
                super::super::windows_metadata::fixture_acl_command(&program, path, grant);
            let out = crate::processes::run(
                command,
                crate::processes::Options {
                    timeout: Some(std::time::Duration::from_secs(20)),
                    output_limit: 2 * 1024 * 1024,
                    stderr_limit: Some(4096),
                    ..Default::default()
                },
            )?
            .complete()?;
            if !out.status.success() {
                return Err(format!(
                    "Owned DACL fixture failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
            Ok(())
        };
        let temp = crate::git::file_comparison::Scratch::new()?;
        for phase in [
            Phase::BeforeMove,
            Phase::AfterMove,
            Phase::BeforeInstall,
            Phase::AfterInstall,
        ] {
            let path = temp.0.join(format!("dacl-{phase:?}"));
            write(&path, b"original")?;
            change(&path, true)?;
            let output = Output::capture(&path, &[])?;
            let original = output.original.as_ref().ok_or("No output")?;
            let identity = original.handle.as_ref().ok_or("No output identity")?;
            let original_stamp = original.stamp.clone();
            let mut hook_error = None;
            let result = output.save_with(b"candidate", &[], &mut |at| {
                if at == phase {
                    let target = if phase == Phase::BeforeMove {
                        Ok(path.clone())
                    } else {
                        std::fs::read_dir(&temp.0)
                            .map_err(|e| e.to_string())
                            .and_then(|entries| {
                                entries
                                    .filter_map(Result::ok)
                                    .map(|e| e.path().join("previous"))
                                    .find(|p| {
                                        same_file::Handle::from_path(p)
                                            .is_ok_and(|h| &h == identity.as_ref())
                                    })
                                    .ok_or("No displaced source".into())
                            })
                    };
                    if let Err(e) = target.and_then(|target| change(&target, false)) {
                        hook_error = Some(e);
                    }
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(
                result.is_err(),
                "{phase:?} must reject DACL-only tightening"
            );
            assert_eq!(
                Some(super::super::files::Stamp::new(
                    &identity.as_file().metadata().map_err(|e| e.to_string())?
                )),
                original_stamp,
                "DACL fixture must preserve ordinary guarded file fields"
            );
            if phase != Phase::AfterInstall {
                assert_eq!(read(&path)?, b"original");
            }
        }
        Ok(())
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "requires an owned Windows fixture session with SeSecurityPrivilege; run explicitly after recording gates"]
    fn native_windows_label_and_audit_only_drift_are_refused_and_full_copy_is_equal()
    -> Result<(), String> {
        let root = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot unavailable")?);
        if !root.is_absolute() {
            return Err("Absolute SystemRoot required".into());
        }
        let temp = crate::git::file_comparison::Scratch::new()?;
        for audit in [false, true] {
            for phase in [
                Phase::BeforeMove,
                Phase::AfterMove,
                Phase::BeforeInstall,
                Phase::AfterInstall,
            ] {
                let path = temp.0.join(format!("full-{audit}-{phase:?}"));
                write(&path, b"original")?;
                let output = Output::capture(&path, &[])?;
                let original = output.original.as_ref().ok_or("original")?;
                let identity = original.handle.as_ref().ok_or("identity")?;
                let before = super::super::windows_metadata::snapshot(&path)?;
                let mut hook_error = None;
                let result = output.save_with(b"candidate", &[], &mut |at| {
                    if at != phase {
                        return;
                    }
                    let target = if phase == Phase::BeforeMove {
                        Ok(path.clone())
                    } else {
                        std::fs::read_dir(&temp.0)
                            .map_err(|e| e.to_string())
                            .and_then(|entries| {
                                entries
                                    .filter_map(Result::ok)
                                    .map(|e| e.path().join("previous"))
                                    .find(|p| {
                                        same_file::Handle::from_path(p)
                                            .is_ok_and(|h| &h == identity.as_ref())
                                    })
                                    .ok_or("No displaced original".into())
                            })
                    };
                    let changed = target.and_then(|target| {
                        let program = if audit {
                            root.join("System32/WindowsPowerShell/v1.0/powershell.exe")
                        } else {
                            root.join("System32/icacls.exe")
                        };
                        let command = super::super::windows_metadata::fixture_policy_command(
                            &program, &target, audit,
                        );
                        let out = crate::processes::run(
                            command,
                            crate::processes::Options {
                                timeout: Some(std::time::Duration::from_secs(20)),
                                ..Default::default()
                            },
                        )?
                        .complete()?;
                        if !out.status.success() {
                            return Err(format!(
                                "Owned full-policy fixture failed: {}",
                                String::from_utf8_lossy(&out.stderr)
                            ));
                        }
                        if super::super::windows_metadata::snapshot(&target)? == before {
                            return Err("Native label/audit did not change".into());
                        }
                        Ok(())
                    });
                    if let Err(e) = changed {
                        hook_error = Some(e);
                    }
                });
                if let Some(e) = hook_error {
                    return Err(e);
                }
                assert!(
                    result.is_err(),
                    "{audit} {phase:?} must refuse full-policy drift"
                );
                if phase != Phase::AfterInstall {
                    assert_eq!(read(&path)?, b"original");
                }
            }
            let source = temp.0.join(format!("full-copy-{audit}"));
            write(&source, b"original")?;
            let program = if audit {
                root.join("System32/WindowsPowerShell/v1.0/powershell.exe")
            } else {
                root.join("System32/icacls.exe")
            };
            let out = crate::processes::run(
                super::super::windows_metadata::fixture_policy_command(&program, &source, audit),
                crate::processes::Options::default(),
            )?
            .complete()?;
            assert!(out.status.success());
            let before = super::super::windows_metadata::snapshot(&source)?;
            let saved = Output::capture(&source, &[])?.save(b"new", &[])?;
            assert_eq!(super::super::windows_metadata::snapshot(&source)?, before);
            assert!(saved.previous.is_some());
        }
        Ok(())
    }
}
