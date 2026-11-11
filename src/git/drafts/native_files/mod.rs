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
        Err(crate::i18n::text(
            "backend-full-native-working-state-policy-is-unavailable-on-this-platform-03a82e",
        )
        .into())
    }
    #[cfg(unix)]
    fn install(&self, file: &File, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if self.platform != std::env::consts::OS {
            return Err(crate::i18n::text(
                "backend-saved-access-policy-belongs-to-another-platform-no-public-file-ch-40c5c1",
            )
            .into());
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
            return Err(crate::i18n::text(
                "backend-full-access-policy-could-not-be-preserved-on-the-private-candidat-0122a6",
            )
            .into());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    fn install(&self, _: &File, _: &Path) -> Result<(), String> {
        Err(crate::i18n::text("backend-native-policy-restoration-unavailable-3fe736").into())
    }
    #[cfg(any(unix, test))]
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.platform != std::env::consts::OS || self.mode > 0o7777 {
            return Err(crate::i18n::text(
                "backend-saved-full-access-policy-is-unsupported-on-this-platform-f74459",
            )
            .into());
        }
        Ok(())
    }
    #[cfg(any(unix, test))]
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

pub(super) fn policy_supported(repo: &Path) -> bool {
    !crate::remote::is_remote(repo) && cfg!(any(target_os = "macos", target_os = "linux"))
}

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
            return Err(crate::i18n::text("backend-regular-no-follow-file-required-63382e").into());
        }
        let path = dir.path.join(name);
        let before = Policy::read(&file, &path)?;
        let mut bytes = Vec::new();
        (&file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err(crate::i18n::text(
                "backend-file-exceeds-bounded-snapshot-allowance-bb90f5",
            )
            .into());
        }
        let after = Policy::read(&file, &path)?;
        if before != after || !dir.unchanged() {
            return Err(crate::i18n::text(
                "backend-file-policy-or-directory-changed-while-snapshotting-368aab",
            )
            .into());
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
    Err(crate::i18n::text("backend-cannot-create-a-private-recovery-directory-faca9e").into())
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
            return Err(crate::i18n::text("backend-parent-directory-changed-9ad539").into());
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
            let snapshot = Snapshot::read(&recovery, "candidate", bytes.len())?.ok_or(
                crate::i18n::text("backend-private-candidate-disappeared-5b38b0"),
            )?;
            if snapshot.identity != identity || snapshot.bytes != bytes {
                return Err(crate::i18n::text("backend-private-candidate-changed-a23c2e").into());
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
            return Err(crate::i18n::message(
                "backend-directory-moved-recovery-09ef49",
                &[("evidence", (self.evidence()).to_string().into())],
            ));
        }
        if let Some(before) = &self.before {
            if !before.same_at(&self.parent, &self.name) {
                return Err(crate::i18n::text(
                    "backend-file-policy-changed-before-publication-7164b3",
                )
                .into());
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
                return Err(crate::i18n::message(
                    "backend-captured-a-concurrent-edit-create-only-restoration-restored-evide-fba254",
                    &[
                        ("restored", format!("{:?}", restored).into()),
                        ("evidence", (self.evidence()).to_string().into()),
                    ],
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
                    crate::i18n::message(
                        "backend-create-only-installation-refused-e-evidence-501da1",
                        &[
                            ("detail", (e).to_string().into()),
                            ("evidence", (self.evidence()).to_string().into()),
                        ],
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
            return Err(crate::i18n::message(
                "backend-publication-changed-retain-all-evidence-a3486e",
                &[("evidence", (self.evidence()).to_string().into())],
            ));
        }
        Ok(())
    }
    pub(super) fn rollback(&mut self) -> Result<(), String> {
        if !self.applied {
            return Ok(());
        }
        if !self.parent.unchanged() || !self.recovery.unchanged() {
            return Err(crate::i18n::message(
                "backend-rollback-parent-identity-moved-no-pathname-based-restoration-atte-3c6f70",
                &[("evidence", (self.evidence()).to_string().into())],
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
                return Err(crate::i18n::message(
                    "backend-rollback-capture-refused-e-evidence-96ecb6",
                    &[
                        ("detail", (e).to_string().into()),
                        ("evidence", (self.evidence()).to_string().into()),
                    ],
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
                            crate::i18n::message(
                                "backend-create-only-rollback-refused-e-evidence-cacf21",
                                &[
                                    ("detail", (e).to_string().into()),
                                    ("evidence", (self.evidence()).to_string().into()),
                                ],
                            )
                        })?;
                }
                let restored = match &self.before {
                    Some(before) => before.same_at(&self.parent, &self.name),
                    None => Snapshot::read(&self.parent, &self.name, super::FILE_LIMIT)?.is_none(),
                };
                if !restored {
                    return Err(crate::i18n::message(
                        "backend-rollback-postcheck-changed-evidence-cf46e2",
                        &[("evidence", (self.evidence()).to_string().into())],
                    ));
                }
                if let Some(candidate) = &self.candidate
                    && !candidate.same_at(&self.recovery, "rollback-current")
                {
                    return Err(crate::i18n::message(
                        "backend-captured-candidate-was-edited-during-rollback-editor-bytes-retain-11f70f",
                        &[("evidence", (self.evidence()).to_string().into())],
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
                Err(crate::i18n::message(
                    "backend-concurrent-editor-save-retained-create-only-restoration-restored-9fb8f4",
                    &[
                        ("restored", format!("{:?}", restored).into()),
                        ("evidence", (self.evidence()).to_string().into()),
                        ("evidence3", (self.evidence()).to_string().into()),
                    ],
                ))
            }
            Restore::Retain => Err(crate::i18n::message(
                "backend-rollback-outcome-needs-inspection-captured-entry-and-original-ret-4668fe",
                &[("evidence", (self.evidence()).to_string().into())],
            )),
        }
    }
}
// Drop deliberately performs no mutation. Evidence is retained even after success;
// callers expose its path instead of racing public cleanup or discarding undo policy.
#[cfg(test)]
mod tests;

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
    if !policy_supported(repo) {
        return Ok(Policies::new());
    }
    #[cfg(unix)]
    {
        let identity = crate::git::recovery::pinned_worktree(repo, active)?;
        let pin = identity.root_pin.as_ref().ok_or(crate::i18n::text(
            "backend-missing-working-directory-identity-0b73c9",
        ))?;
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
                    .ok_or(crate::i18n::text("backend-missing-parent-27f5e2"))?
                    .to_owned();
                let dir = Directory::from_file(parent, held).map_err(|e| e.to_string())?;
                let snapshot = Snapshot::read(&dir, &name, bytes.len())?
                    .ok_or(crate::i18n::text("backend-working-file-disappeared-c0cb63"))?;
                if snapshot.bytes != *bytes
                    || (snapshot.policy.mode() & 0o111 != 0) != (mode == "100755")
                {
                    return Err(crate::i18n::message(
                        "backend-path-changed-while-capturing-full-access-policy-52aab5",
                        &[("path", (path).to_string().into())],
                    ));
                }
                policies.insert(path.clone(), snapshot.policy);
            }
        }
        Ok(policies)
    }
    #[cfg(not(unix))]
    {
        let _ = (contents, active);
        Err(crate::i18n::text("backend-native-working-access-policy-unavailable-355b03").into())
    }
}

#[cfg(test)]
mod saved_policy_tests;
