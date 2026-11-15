//! An allocation is owned only when its private marker and parent registry agree.
//! Metadata lives outside `checkout`, so test commands see only their indexed files.
use super::{ScratchName, allocation, this_machine};
use crate::remote;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    path::{Path, PathBuf},
};
const MARKER: &str = "gitcraft-owner.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Ownership {
    allocation: String,
    token: String,
    created: u64,
    machine: String,
    pid: u32,
    boot: Option<String>,
}
fn boot() -> Option<String> {
    // Unknown platforms use the conservative live-PID check; no boot is invented.
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().into())
}
fn registry(dir: &Path) -> Option<PathBuf> {
    Some(remote::join(
        dir.parent()?,
        &format!(".owner-{}", dir.file_name()?.to_str()?),
    ))
}
fn read(path: &Path) -> Option<Ownership> {
    let bytes = if remote::is_remote(path) {
        let parent = path.parent()?;
        remote::read_within(parent, path.file_name()?.to_str()?, 4096)?.ok()??
    } else {
        use std::io::Read;
        let metadata = std::fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() || metadata.is_symlink() || metadata.len() > 4096 {
            return None;
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .ok()?
            .take(4097)
            .read_to_end(&mut bytes)
            .ok()?;
        bytes
    };
    if bytes.len() > 4096 {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}
impl Ownership {
    pub(super) fn create(dir: &Path) -> Result<Self, String> {
        let allocation = dir
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid scratch allocation")?;
        let owner = Self {
            allocation: allocation.into(),
            token: format!(
                "{:016x}{:016x}",
                RandomState::new().hash_one(()),
                RandomState::new().hash_one(())
            ),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
            machine: this_machine(),
            pid: std::process::id(),
            boot: boot(),
        };
        let bytes = serde_json::to_vec(&owner).map_err(|e| e.to_string())?;
        // The allocation was exclusively created with mode 0700. The registry is
        // create-new too: never replace an older allocation's ownership record.
        let record = registry(dir).ok_or("invalid scratch registry")?;
        write_new(&record, &bytes)?;
        if let Err(e) = write_new(&remote::join(dir, MARKER), &bytes) {
            let _ = remote::remove_file(&record);
            return Err(e);
        }
        Ok(owner)
    }
    pub(super) fn verified(dir: &Path) -> Option<Self> {
        let owner = read(&remote::join(dir, MARKER))?;
        let name = ScratchName::parse(dir.file_name()?.to_str()?)?;
        (read(&registry(dir)?)? == owner
            && owner.allocation == name.to_string()
            && owner.machine == name.machine
            && owner.pid == name.pid
            && owner.token.len() == 32
            && owner.created > 0)
            .then_some(owner)
    }
    pub(super) fn matches(&self, dir: &Path) -> bool {
        Self::verified(dir).as_ref() == Some(self)
    }
    fn abandoned(
        &self,
        machine: &str,
        current_boot: Option<&str>,
        alive: impl FnOnce(u32) -> bool,
    ) -> bool {
        self.machine == machine
            && match (self.boot.as_deref(), current_boot) {
                (Some(then), Some(now)) if then != now => true,
                _ => !alive(self.pid),
            }
    }
    pub(super) fn dead(&self) -> bool {
        self.abandoned(&this_machine(), boot().as_deref(), super::process_alive)
    }
    pub(super) fn finish(&self, dir: &Path) -> Result<(), String> {
        if !self.matches(dir) {
            return Err("scratch ownership could not be verified".into());
        }
        remote::remove_file(&remote::join(dir, MARKER)).map_err(|e| e.to_string())?;
        allocation::discard_empty(dir);
        remote::remove_file(&registry(dir).ok_or("invalid scratch registry")?)
            .map_err(|e| e.to_string())
    }
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(target) = remote::parse(path) {
        let request = remote::Request {
            script: format!("umask 077\nset -C\ncat > {}", remote::sh_path(&target.path)),
            stdin: Some(bytes.to_vec()),
            timeout: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        };
        let out = remote::exec(&target.host, &request)?;
        return if out.code == 0 {
            Ok(())
        } else {
            Err("could not create scratch ownership record".into())
        };
    }
    use std::io::Write;
    std::fs::File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| {
            f.write_all(bytes)?;
            f.sync_all()
        })
        .map_err(|e| e.to_string())
}

/// Only owned allocations; a matching legacy suffix alone grants no permission.
pub(super) fn candidates(repo: &Path, root: &Path) -> Vec<PathBuf> {
    if !remote::is_remote(root) {
        return std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_type().ok().filter(|t| t.is_dir()).map(|_| e.path()))
            .collect();
    }
    super::run(Some(repo), &["worktree", "list", "--porcelain"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(|p| remote::resolve(repo, p))
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .filter(|p| p.parent() == Some(root))
        .collect()
}

#[cfg(test)]
mod tests;
