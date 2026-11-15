//! Exclusive empty destinations. A failed Git add never authorizes force removal.
use super::{ScratchName, this_machine};
use crate::remote;
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    path::{Path, PathBuf},
};

pub(super) fn reserve(root: &Path, label: &str) -> Result<PathBuf, String> {
    if !super::LABELS.contains(&label) {
        return Err("unsupported scratch purpose".into());
    }
    remote::create_dir_all(root).map_err(|e| e.to_string())?;
    for _ in 0..8 {
        crate::processes::deadline::check()?;
        let name = ScratchName {
            label: label.into(),
            machine: this_machine(),
            pid: std::process::id(),
            n: RandomState::new().hash_one(()),
        };
        let path = remote::join(root, &name.to_string());
        if claim(&path)? {
            return Ok(path);
        }
    }
    Err("could not reserve a unique scratch destination".into())
}
fn directory_command(path: &Path, command: &str) -> Result<i32, String> {
    let target = remote::parse(path).ok_or("expected a remote scratch path")?;
    let request = remote::Request {
        script: format!("umask 077\n{command} -- {}", remote::sh_path(&target.path)),
        timeout: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    };
    remote::exec(&target.host, &request).map(|out| out.code)
}
fn claim(path: &Path) -> Result<bool, String> {
    if remote::is_remote(path) {
        return directory_command(path, "mkdir").map(|code| code == 0);
    }
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}
/// Only rmdir: partial registration/files survive an uncertain add for crash recovery.
pub(super) fn discard_empty(path: &Path) {
    crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
        crate::processes::deadline::cleanup(|| {
            if remote::is_remote(path) {
                let _ = directory_command(path, "rmdir");
            } else {
                let _ = std::fs::remove_dir(path);
            }
        });
    });
}
#[cfg(test)]
mod tests;
