//! Preserve the displaced inode, then install create-only. No blind destination overwrite.
use super::directory::Directory;
use super::files::{FILE_LIMIT, Snapshot, label};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod reservation;
use reservation::{Lock, Recovery};

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
                recovery.retain();
                hook(Phase::AfterMove);
                let displaced = recovery
                    .dir()?
                    .snapshot(previous, &recovery.dir()?.path.join(previous));
                let displaced = match displaced {
                    Ok(snapshot) => snapshot,
                    Err(e) => {
                        recovery.restore(&self.parent, &self.name);
                        return Err(recovery.error(&e));
                    }
                };
                if !same_optional(Some(&displaced), self.original.as_ref())
                    || !self.parent_current()
                {
                    // Restore only into absence. A concurrent creator keeps its own file.
                    recovery.restore(&self.parent, &self.name);
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
                    recovery.restore(&self.parent, &self.name);
                }
                return Err(recovery.error(crate::i18n::text(
                    "interaction-standalone-error-before-install",
                )));
            }
            if let Err(e) = recovery.dir()?.link(candidate, &self.parent, &self.name) {
                if backup.is_some() {
                    recovery.restore(&self.parent, &self.name);
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

#[cfg(test)]
mod tests;
