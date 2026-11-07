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
}

impl Output {
    pub fn initial_bytes(&self) -> Option<&[u8]> {
        self.original.as_ref().map(|s| s.bytes.as_slice())
    }
    pub fn capture(path: &Path, inputs: &[Snapshot]) -> Result<Self, String> {
        if super::files::null_input(path) {
            return Err("An empty-input device cannot be the merged output".into());
        }
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .ok_or("The merged result needs a file name")?
            .to_owned();
        let requested_parent = path
            .parent()
            .ok_or("No parent for merged result")?
            .to_owned();
        let parent = std::fs::canonicalize(&requested_parent)
            .map_err(|e| format!("Could not open output parent: {e}"))?;
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
                return Err("MERGED aliases an input file; choose a separate output so inputs remain read-only".into());
            }
            if original
                .handle
                .as_ref()
                .ok_or("No output identity")?
                .as_file()
                .metadata()
                .map_err(|e| e.to_string())?
                .permissions()
                .readonly()
            {
                return Err("The merged output is read-only".into());
            }
        }
        Ok(output)
    }
    fn current(&self) -> Result<Option<Snapshot>, String> {
        match self.parent.read(&self.name) {
            Ok(file) => Snapshot::from_file(&self.path, file, &|| true).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!(
                "MERGED must be a plain file, not a link/device: {e}"
            )),
        }
    }
    fn guard(&self, inputs: &[Snapshot]) -> Result<(), String> {
        if !self.parent_current() {
            return Err("The output parent changed identity; reopen the file tool".into());
        }
        if !same_optional(self.current()?.as_ref(), self.original.as_ref()) {
            return Err("MERGED changed since opening; reopen before saving. The current file was preserved.".into());
        }
        for input in inputs {
            if !input.unchanged()? {
                return Err(format!(
                    "Input {} changed; reopen before saving",
                    label(&input.path)
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
            return Err("Merged output exceeds 2 MiB".into());
        }
        self.guard(inputs)?;
        let _lock = Lock::new(Arc::clone(&self.parent), &self.name)?;
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
        let prepared = Snapshot::from_file(
            &recovery.dir()?.path.join(candidate),
            recovery.dir()?.read(candidate).map_err(|e| e.to_string())?,
            &|| true,
        )?;
        // Probe before moving MERGED. Unsupported hard-link filesystems fail with
        // the original destination untouched; there is deliberately no copy fallback.
        recovery
            .dir()?
            .link(candidate, recovery.dir()?, OsStr::new("probe"))
            .map_err(|e| {
                format!("This filesystem cannot install a result atomically without overwrite: {e}")
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
                .map_err(|e| format!("Could not preserve the previous result: {e}"))?;
            recovery.keep = true;
            hook(Phase::AfterMove);
            let displaced = recovery
                .dir()?
                .read(previous)
                .map_err(|e| e.to_string())
                .and_then(|file| {
                    Snapshot::from_file(
                        &recovery.parent.path.join(&recovery.name).join(previous),
                        file,
                        &|| true,
                    )
                });
            let displaced = match displaced {
                Ok(snapshot) => snapshot,
                Err(e) => {
                    recovery.restore(self);
                    return Err(recovery.error(&e));
                }
            };
            if !same_optional(Some(&displaced), self.original.as_ref()) || !self.parent_current() {
                // Restore only into absence. A concurrent creator keeps its own file.
                recovery.restore(self);
                return Err(recovery.error(
                    "MERGED or its parent changed during saving; no successful save was reported",
                ));
            }
            Some(recovery.dir()?.path.join(previous))
        } else {
            None
        };
        hook(Phase::BeforeInstall);
        if !self.parent_current() || inputs.iter().any(|i| i.unchanged() != Ok(true)) {
            if backup.is_some() {
                recovery.restore(self);
            }
            return Err(recovery.error("The output parent changed before installation"));
        }
        if let Err(e) = recovery.dir()?.link(candidate, &self.parent, &self.name) {
            if backup.is_some() {
                recovery.restore(self);
            }
            return Err(recovery.error(&format!(
                "Could not install without overwrite; a concurrent destination was preserved: {e}"
            )));
        }
        hook(Phase::AfterInstall);
        let installed = self.current().map_err(|e| recovery.error(&e))?;
        if !same_optional(installed.as_ref(), Some(&prepared))
            || !self.parent_current()
            || inputs.iter().any(|i| i.unchanged() != Ok(true))
        {
            return Err(recovery.error("Files changed during the save. The current result and displaced bytes were preserved; inspect them before continuing"));
        }
        Ok(Saved { previous: backup })
    }
}
fn same_optional(a: Option<&Snapshot>, b: Option<&Snapshot>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.handle == b.handle && a.stamp == b.stamp && a.bytes == b.bytes,
        _ => false,
    }
}

struct Lock {
    parent: Arc<Directory>,
    name: OsString,
    handle: same_file::Handle,
}
impl Lock {
    fn new(parent: Arc<Directory>, target: &OsStr) -> Result<Self, String> {
        let name = OsString::from(format!(
            ".gitcraft-merge-{:x}.lock",
            md5::compute(target.as_encoded_bytes())
        ));
        let file = parent.create(&name, &[]).map_err(|e| {
            format!("Could not reserve the result (another save may own its lock): {e}")
        })?;
        Ok(Self {
            parent,
            name,
            handle: same_file::Handle::from_file(file).map_err(|e| e.to_string())?,
        })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        if self
            .parent
            .read(&self.name)
            .and_then(same_file::Handle::from_file)
            .is_ok_and(|h| h == self.handle)
        {
            let _ = self.parent.remove_file(&self.name);
        }
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
        Err("Could not reserve a recovery directory".into())
    }
    fn dir(&self) -> Result<&Directory, String> {
        self.directory
            .as_ref()
            .ok_or_else(|| "The recovery directory is closed".into())
    }
    fn error(&self, message: &str) -> String {
        format!(
            "{message}. Recovery directory: {}",
            label(&self.parent.path.join(&self.name))
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
}
