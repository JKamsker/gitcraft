//! Owns reservation identity, private recovery entries and their teardown.
use super::{Directory, Phase, label};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    ffi::{OsStr, OsString},
    io,
    sync::Arc,
};

pub(super) struct Lock {
    parent: Arc<Directory>,
    recovery: Recovery,
    name: OsString,
    handle: Option<same_file::Handle>,
    active: bool,
}
impl Lock {
    #[cfg(test)]
    pub(super) fn name(&self) -> &OsStr {
        &self.name
    }
    #[cfg(test)]
    pub(super) fn recovery_path(&self) -> Result<std::path::PathBuf, String> {
        Ok(self.recovery.dir()?.path.clone())
    }

    pub(super) fn new(parent: Arc<Directory>, target: &OsStr) -> Result<Self, String> {
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
    pub(super) fn finish<T>(
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
pub(super) struct Recovery {
    parent: Arc<Directory>,
    name: OsString,
    directory: Option<Directory>,
    keep: bool,
}
impl Recovery {
    pub(super) fn retain(&mut self) {
        self.keep = true;
    }

    pub(super) fn new(parent: Arc<Directory>) -> Result<Self, String> {
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
    pub(super) fn dir(&self) -> Result<&Directory, String> {
        self.directory
            .as_ref()
            .ok_or_else(|| crate::i18n::text("interaction-standalone-error-recovery-closed").into())
    }
    pub(super) fn error(&self, message: &str) -> String {
        crate::i18n::message(
            "interaction-standalone-error-recovery-location",
            &[
                ("detail", (message).into()),
                ("path", (label(&self.parent.path.join(&self.name))).into()),
            ],
        )
    }
    pub(super) fn restore(&self, parent: &Directory, name: &OsStr) {
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
        let _ = dir.link(OsStr::new("previous"), parent, name);
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
