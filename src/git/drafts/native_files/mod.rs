//! Working-state publication derived from fileviews 1535d831's held-directory,
//! capture/inspect and create-only restoration protocol. Private recovery directories
//! are retained: no public-name check/unlink and no rollback EXCHANGE is used.
mod access;
mod directory;
pub(super) use directory::Directory;
use serde::{Deserialize, Serialize};
use std::{ffi::OsStr, fs::File, io::Read, path::Path, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Policy {
    platform: String,
    mode: u32,
    uid: u32,
    gid: u32,
    access: access::AccessMetadata,
}
impl Policy {
    #[cfg(unix)]
    fn read(file: &File, path: &Path) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        Ok(Self {
            platform: std::env::consts::OS.into(),
            mode: meta.mode() & 0o7777,
            uid: meta.uid(),
            gid: meta.gid(),
            access: access::read(file, path)?,
        })
    }
    #[cfg(not(unix))]
    fn read(_: &File, _: &Path) -> Result<Self, String> {
        Err("Full native working-state policy is unavailable on this platform".into())
    }
    #[cfg(unix)]
    fn install(&self, file: &File, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if self.platform != std::env::consts::OS {
            return Err(
                "Saved access policy belongs to another platform; no public file changed".into(),
            );
        }
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if meta.uid() != self.uid || meta.gid() != self.gid {
            rustix::fs::fchown(
                file,
                Some(rustix::process::Uid::from_raw(self.uid)),
                Some(rustix::process::Gid::from_raw(self.gid)),
            )
            .map_err(|e| e.to_string())?;
        }
        access::copy_index_policy(file, path, &self.access, path)?;
        file.set_permissions(std::fs::Permissions::from_mode(self.mode))
            .map_err(|e| e.to_string())?;
        if Self::read(file, path)? != *self {
            return Err(
                "Full access policy could not be preserved on the private candidate".into(),
            );
        }
        Ok(())
    }
    #[cfg(not(unix))]
    fn install(&self, _: &File, _: &Path) -> Result<(), String> {
        Err("Native policy restoration unavailable".into())
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.platform != std::env::consts::OS || self.mode > 0o7777 {
            return Err("Saved full access policy is unsupported on this platform".into());
        }
        Ok(())
    }
    pub(super) fn mode(&self) -> u32 {
        self.mode
    }
    pub(super) fn with_git_mode(&self, mode: &str) -> Self {
        let mut value = self.clone();
        value.mode = if mode == "100755" {
            value.mode | ((value.mode & 0o444) >> 2)
        } else {
            value.mode & !0o111
        };
        value
    }
}
pub(super) type Policies = std::collections::BTreeMap<String, Policy>;
#[derive(Debug)]
pub(super) struct Snapshot {
    identity: same_file::Handle,
    pub bytes: Vec<u8>,
    pub policy: Policy,
}
impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity && self.bytes == other.bytes && self.policy == other.policy
    }
}
impl Eq for Snapshot {}
impl Snapshot {
    #[cfg(unix)]
    pub(super) fn identity_token(&self) -> Result<String, String> {
        crate::git::recovery::native_file_identity(self.identity.as_file()).map(|id| id.0)
    }
    pub(super) fn read(dir: &Directory, name: &str, limit: usize) -> Result<Option<Self>, String> {
        let file = match dir.read(OsStr::new(name)) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Regular no-follow file required".into());
        }
        let path = dir.path.join(name);
        let before = Policy::read(&file, &path)?;
        let mut bytes = Vec::new();
        (&file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err("File exceeds bounded snapshot allowance".into());
        }
        let after = Policy::read(&file, &path)?;
        if before != after || !dir.unchanged() {
            return Err("File policy or directory changed while snapshotting".into());
        }
        Ok(Some(Self {
            identity: same_file::Handle::from_file(file).map_err(|e| e.to_string())?,
            bytes,
            policy: after,
        }))
    }
    pub(super) fn same_at(&self, dir: &Directory, name: &str) -> bool {
        Self::read(dir, name, self.bytes.len())
            .ok()
            .flatten()
            .as_ref()
            == Some(self)
    }
}
fn recovery(parent: &Directory) -> Result<Arc<Directory>, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    for _ in 0..100 {
        let name = format!(
            ".gitcraft-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        match parent.child(OsStr::new(&name)) {
            Ok(dir) => return Ok(Arc::new(dir)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Cannot create a private recovery directory".into())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Captured {
    Owned,
    Foreign,
    Unreadable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Restore {
    Original,
    Captured,
    Retain,
}
fn restoration(captured: Captured, original_matches: bool) -> Restore {
    match captured {
        Captured::Owned if original_matches => Restore::Original,
        Captured::Foreign | Captured::Unreadable => Restore::Captured,
        _ => Restore::Retain,
    }
}
pub(super) struct Replacement {
    pub parent: Arc<Directory>,
    pub recovery: Arc<Directory>,
    pub name: String,
    before: Option<Snapshot>,
    candidate: Option<Snapshot>,
    applied: bool,
}
impl Replacement {
    pub(super) fn prepare(
        parent: Arc<Directory>,
        name: String,
        before: Option<Snapshot>,
        bytes: Option<&[u8]>,
        policy: Option<&Policy>,
        git_mode: Option<&str>,
    ) -> Result<Self, String> {
        if !parent.unchanged() {
            return Err("Parent directory changed".into());
        }
        let recovery = recovery(&parent)?;
        let candidate = if let Some(bytes) = bytes {
            let file = recovery
                .create(OsStr::new("candidate"), bytes)
                .map_err(|e| e.to_string())?;
            let identity =
                same_file::Handle::from_file(file.try_clone().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let original_policy = policy
                .cloned()
                .unwrap_or(Policy::read(&file, &recovery.path.join("candidate"))?);
            let desired = git_mode.map_or(original_policy.clone(), |mode| {
                original_policy.with_git_mode(mode)
            });
            desired.install(&file, &recovery.path.join("candidate"))?;
            let snapshot = Snapshot::read(&recovery, "candidate", bytes.len())?
                .ok_or("Private candidate disappeared")?;
            if snapshot.identity != identity || snapshot.bytes != bytes {
                return Err("Private candidate changed".into());
            }
            Some(snapshot)
        } else {
            None
        };
        Ok(Self {
            parent,
            recovery,
            name,
            before,
            candidate,
            applied: false,
        })
    }
    pub(super) fn evidence(&self) -> String {
        self.recovery.path.display().to_string()
    }
    pub(super) fn publish(&mut self) -> Result<(), String> {
        if !self.parent.unchanged() || !self.recovery.unchanged() {
            return Err(format!("Directory moved; recovery {}", self.evidence()));
        }
        if let Some(before) = &self.before {
            if !before.same_at(&self.parent, &self.name) {
                return Err("File/policy changed before publication".into());
            }
            self.parent
                .move_file(
                    OsStr::new(&self.name),
                    &self.recovery,
                    OsStr::new("original"),
                )
                .map_err(|e| e.to_string())?;
            self.applied = true;
            if !before.same_at(&self.recovery, "original") {
                let restored = self.recovery.link(
                    OsStr::new("original"),
                    &self.parent,
                    OsStr::new(&self.name),
                );
                return Err(format!(
                    "Captured a concurrent edit; create-only restoration {restored:?}; evidence {}",
                    self.evidence()
                ));
            }
        }
        if self.candidate.is_some() {
            self.recovery
                .link(
                    OsStr::new("candidate"),
                    &self.parent,
                    OsStr::new(&self.name),
                )
                .map_err(|e| {
                    format!(
                        "Create-only installation refused: {e}; evidence {}",
                        self.evidence()
                    )
                })?;
            self.applied = true;
        }
        self.verify_installed()
    }
    pub(super) fn verify_installed(&self) -> Result<(), String> {
        let same = match &self.candidate {
            Some(candidate) => candidate.same_at(&self.parent, &self.name),
            None => Snapshot::read(&self.parent, &self.name, super::FILE_LIMIT)?.is_none(),
        };
        let original = self
            .before
            .as_ref()
            .is_none_or(|s| s.same_at(&self.recovery, "original"));
        if !same || !original || !self.parent.unchanged() || !self.recovery.unchanged() {
            return Err(format!(
                "Publication changed; retain all evidence {}",
                self.evidence()
            ));
        }
        Ok(())
    }
    pub(super) fn rollback(&mut self) -> Result<(), String> {
        if !self.applied {
            return Ok(());
        }
        if !self.parent.unchanged() || !self.recovery.unchanged() {
            return Err(format!(
                "Rollback parent identity moved; no pathname-based restoration attempted. Evidence is retained in the held directory formerly at {}",
                self.evidence()
            ));
        }
        // Capture first. Classification examines the entry actually displaced,
        // including in-place changes; it never trusts an earlier pathname check.
        let captured = match self.parent.move_file(
            OsStr::new(&self.name),
            &self.recovery,
            OsStr::new("rollback-current"),
        ) {
            Ok(()) => match Snapshot::read(&self.recovery, "rollback-current", super::FILE_LIMIT) {
                Ok(Some(now)) if self.candidate.as_ref() == Some(&now) => Captured::Owned,
                Ok(Some(_)) => Captured::Foreign,
                _ => Captured::Unreadable,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && self.candidate.is_none() => {
                Captured::Owned
            }
            Err(e) => {
                return Err(format!(
                    "Rollback capture refused: {e}; evidence {}",
                    self.evidence()
                ));
            }
        };
        let original = self
            .before
            .as_ref()
            .is_none_or(|s| s.same_at(&self.recovery, "original"));
        match restoration(captured, original) {
            Restore::Original => {
                if self.before.is_some() {
                    self.recovery
                        .link(OsStr::new("original"), &self.parent, OsStr::new(&self.name))
                        .map_err(|e| {
                            format!(
                                "Create-only rollback refused: {e}; evidence {}",
                                self.evidence()
                            )
                        })?;
                }
                let restored = match &self.before {
                    Some(before) => before.same_at(&self.parent, &self.name),
                    None => Snapshot::read(&self.parent, &self.name, super::FILE_LIMIT)?.is_none(),
                };
                if !restored {
                    return Err(format!(
                        "Rollback postcheck changed; evidence {}",
                        self.evidence()
                    ));
                }
                if let Some(candidate) = &self.candidate
                    && !candidate.same_at(&self.recovery, "rollback-current")
                {
                    return Err(format!(
                        "Captured candidate was edited during rollback; editor bytes retained at {}/rollback-current; original restored create-only",
                        self.evidence()
                    ));
                }
                self.applied = false;
                Ok(())
            }
            Restore::Captured => {
                let restored = self.recovery.link(
                    OsStr::new("rollback-current"),
                    &self.parent,
                    OsStr::new(&self.name),
                );
                Err(format!(
                    "Concurrent editor save retained; create-only restoration {restored:?}; exact evidence {}/rollback-current; original backup {}/original",
                    self.evidence(),
                    self.evidence()
                ))
            }
            Restore::Retain => Err(format!(
                "Rollback outcome needs inspection; captured entry and original retained at {}",
                self.evidence()
            )),
        }
    }
}
// Drop deliberately performs no mutation. Evidence is retained even after success;
// callers expose its path instead of racing public cleanup or discarding undo policy.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_native_capture_decisions_preserve_foreign_and_unknown_entries() {
        assert_eq!(restoration(Captured::Owned, true), Restore::Original);
        assert_eq!(restoration(Captured::Owned, false), Restore::Retain);
        assert_eq!(restoration(Captured::Foreign, true), Restore::Captured);
        assert_eq!(restoration(Captured::Foreign, false), Restore::Captured);
        assert_eq!(restoration(Captured::Unreadable, true), Restore::Captured);
        // Pure decision matrix only. Native capture ordering and create-only
        // filesystem interleavings remain held for independent validation.
    }
}

/// A durable ref acknowledgement can be unknown. Callers must retain installed
/// files/index and recovery evidence on Unknown instead of running rollback.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CommitOutcome {
    Committed,
    Unknown(String),
}
impl CommitOutcome {
    pub(super) fn confirmed(self) -> Result<(), String> {
        match self {
            Self::Committed => Ok(()),
            Self::Unknown(message) => Err(message),
        }
    }
}

pub(super) fn capture_policies(
    repo: &Path,
    contents: &super::working_transaction::Contents,
    active: &dyn Fn() -> bool,
) -> Result<Policies, String> {
    if crate::remote::is_remote(repo) || !cfg!(any(target_os = "macos", target_os = "linux")) {
        return Ok(Policies::new());
    }
    #[cfg(unix)]
    {
        let identity = crate::git::recovery::pinned_worktree(repo, active)?;
        let pin = identity
            .root_pin
            .as_ref()
            .ok_or("Missing working directory identity")?;
        let mut policies = Policies::new();
        for (path, expected) in contents {
            if let Some((mode, bytes)) = expected {
                let (held, name) = crate::git::recovery::pinned_parent(
                    Path::new(&pin.physical),
                    Path::new(&identity.common),
                    path,
                    pin,
                )?;
                let parent = Path::new(&pin.physical)
                    .join(path)
                    .parent()
                    .ok_or("Missing parent")?
                    .to_owned();
                let dir = Directory::from_file(parent, held).map_err(|e| e.to_string())?;
                let snapshot =
                    Snapshot::read(&dir, &name, bytes.len())?.ok_or("Working file disappeared")?;
                if snapshot.bytes != *bytes
                    || (snapshot.policy.mode() & 0o111 != 0) != (mode == "100755")
                {
                    return Err(format!("{path} changed while capturing full access policy"));
                }
                policies.insert(path.clone(), snapshot.policy);
            }
        }
        Ok(policies)
    }
    #[cfg(not(unix))]
    {
        let _ = (contents, active);
        Err("Native working access policy unavailable".into())
    }
}

#[cfg(test)]
mod saved_policy_tests {
    use super::*;
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn pure_native_full_policy_roundtrip_keeps_owner_group_native_attributes_and_acl()
    -> Result<(), String> {
        let mut access = serde_json::json!({"entries": [[[117,115,101,114,46,97],[1,2,3]]]});
        #[cfg(target_os = "macos")]
        {
            access["acl"] = serde_json::json!([{"kind":"user","name":"501","perms":["read"],"flags":[],"allow":false}]);
        }
        let policy: Policy = serde_json::from_value(serde_json::json!({
            "platform":std::env::consts::OS,"mode":416,"uid":501,"gid":20,"access":access
        }))
        .map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec(&policy).map_err(|error| error.to_string())?;
        assert_eq!(
            serde_json::from_slice::<Policy>(&bytes).map_err(|error| error.to_string())?,
            policy
        );
        let mut tightened = policy.clone();
        tightened.mode = 0o600;
        assert_ne!(tightened, policy);
        tightened = policy.clone();
        tightened.gid += 1;
        assert_ne!(tightened, policy);
        let mut changed: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        changed["access"]["entries"][0][1] = serde_json::json!([9]);
        assert_ne!(
            serde_json::from_value::<Policy>(changed).map_err(|error| error.to_string())?,
            policy
        );
        assert!(serde_json::from_str::<Policy>("416").is_err());
        #[cfg(target_os = "macos")]
        {
            let mut changed: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            changed["access"]["acl"][0]["allow"] = serde_json::json!(true);
            assert_ne!(
                serde_json::from_value::<Policy>(changed).map_err(|error| error.to_string())?,
                policy
            );
        }
        assert!(policy.validate().is_ok());
        tightened.platform = "foreign platform".into();
        assert!(tightened.validate().is_err());
        let saved: Policies = [("f".into(), policy.clone())].into();
        assert_eq!(
            serde_json::from_slice::<Policies>(
                &serde_json::to_vec(&saved).map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?["f"],
            policy
        );
        Ok(())
    }
    #[test]
    fn pure_native_unknown_receipt_cannot_be_confirmed() -> Result<(), String> {
        assert!(CommitOutcome::Committed.confirmed().is_ok());
        assert!(
            CommitOutcome::Unknown("operation-1: receipt unavailable".into())
                .confirmed()
                .err()
                .ok_or("expected an error")?
                .contains("operation-1")
        );
        Ok(())
    }
}
