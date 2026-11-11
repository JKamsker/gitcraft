//! Retain displaced inodes and publish create-only through a held index parent.
use super::{Lens, lens, transaction};
use crate::standalone::{access, directory::Directory};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Locked,
    Prepared,
    BeforeMove,
    AfterMove,
    BeforeInstall,
    AfterInstall,
    BeforeUnlock,
    DuringUnlock,
}
struct Snapshot {
    identity: same_file::Handle,
    bytes: Vec<u8>,
    policy: access::AccessMetadata,
    mode: u32,
    uid: u32,
    gid: u32,
}
impl Snapshot {
    fn read(dir: &Directory, name: &OsStr) -> Result<Option<Self>, String> {
        let file = match dir.read(name) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        Self::from(file, &dir.path.join(name)).map(Some)
    }
    fn from(file: File, path: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > 16 * 1024 * 1024 {
            return Err(crate::i18n::text(
                "backend-index-is-not-a-regular-file-within-16-mib-e5c832",
            )
            .into());
        }
        let policy = access::read(&file, path)?;
        let mut bytes = Vec::new();
        (&file)
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 16 * 1024 * 1024 || policy != access::read(&file, path)? {
            return Err(crate::i18n::text("backend-index-changed-during-read-c76aff").into());
        }
        #[cfg(unix)]
        let (mode, uid, gid) = (meta.mode(), meta.uid(), meta.gid());
        #[cfg(not(unix))]
        let (mode, uid, gid) = (0, 0, 0);
        Ok(Self {
            identity: same_file::Handle::from_file(file).map_err(|e| e.to_string())?,
            bytes,
            policy,
            mode,
            uid,
            gid,
        })
    }
    fn same(&self, other: &Self) -> bool {
        self.identity == other.identity
            && self.bytes == other.bytes
            && self.policy == other.policy
            && self.mode == other.mode
            && self.uid == other.uid
            && self.gid == other.gid
    }
}
struct Lock {
    parent: Arc<Directory>,
    recovery: Arc<Directory>,
    name: OsString,
    identity: same_file::Handle,
    active: bool,
}
impl Lock {
    fn owned(&self) -> bool {
        self.parent.unchanged()
            && self
                .parent
                .read(&self.name)
                .is_ok_and(|f| same_file::Handle::from_file(f).is_ok_and(|h| h == self.identity))
    }
    fn release(&mut self) -> Result<(), String> {
        self.release_with(&mut || {})
    }
    fn release_with(&mut self, hook: &mut dyn FnMut()) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        if !self.owned() {
            return Err(crate::i18n::text(
                "backend-index-lock-changed-owner-foreign-lock-preserved-f042aa",
            )
            .into());
        }
        // A check followed by rename is NOT identity-conditional. Capture first and
        // inspect what moved. A late foreign creator is retained and restored only
        // into absence; never unlink somebody else's lock.
        hook();
        self.parent
            .move_file(&self.name, &self.recovery, OsStr::new("released-lock"))
            .map_err(|e| e.to_string())?;
        let owned = self
            .recovery
            .read(OsStr::new("released-lock"))
            .is_ok_and(|f| same_file::Handle::from_file(f).is_ok_and(|h| h == self.identity));
        if !owned {
            let _ = self
                .recovery
                .link(OsStr::new("released-lock"), &self.parent, &self.name);
            return Err(crate::i18n::message(
                "backend-foreign-lock-captured-during-release-retained-at-6c84e6",
                &[("path", (self.recovery.path.display()).to_string().into())],
            ));
        }
        Ok(())
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
fn policy(repo: &Path) -> Result<Vec<u8>, String> {
    let out = super::output(repo, &["config", "--get", "core.sharedRepository"], &|| {
        true
    })?;
    if out.code > 1 {
        return Err(crate::i18n::text(
            "backend-could-not-read-configured-index-sharing-policy-b62cd3",
        )
        .into());
    }
    Ok(out.stdout)
}
pub(super) fn apply(
    repo: &Path,
    lens: &Lens,
    patch: &str,
    reverse: bool,
    hook: &mut dyn FnMut(Phase),
) -> Result<String, String> {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    let index = lens::index_path(repo, &|| true)?;
    let parent = Arc::new(
        Directory::open(
            index
                .parent()
                .ok_or(crate::i18n::text("backend-index-has-no-parent-b1c595"))?,
        )
        .map_err(|e| e.to_string())?,
    );
    let name = index
        .file_name()
        .ok_or(crate::i18n::text("backend-index-has-no-name-a4f6d6"))?;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let recovery = loop {
        let name = OsString::from(format!(
            "gitcraft-index-recovery-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        match parent.child(&name) {
            Ok(d) => break Arc::new(d),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let lock_name = OsString::from(format!("{}.lock", name.to_string_lossy()));
    let held = parent.create(&lock_name, &[]).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-lock-the-git-index-e-78eb1c",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let mut lock = Lock {
        parent: parent.clone(),
        recovery: recovery.clone(),
        name: lock_name,
        identity: same_file::Handle::from_file(held).map_err(|e| e.to_string())?,
        active: true,
    };
    let result = (|| {
        hook(Phase::Locked);
        transaction::verify(repo, lens)?;
        let original = Snapshot::read(&parent, name)?;
        let sharing = policy(repo)?;
        let candidate = OsStr::new("candidate");
        let candidate_path = recovery.path.join(candidate);
        if let Some(s) = &original {
            recovery
                .create(candidate, &s.bytes)
                .map_err(|e| e.to_string())?;
        } else {
            transaction::alternate(repo, &candidate_path, &["read-tree", "--empty"])?;
        }
        if lens.intent_to_add {
            transaction::alternate(
                repo,
                &candidate_path,
                &["update-index", "--force-remove", "--", &lens.spec.path],
            )?;
        }
        let patch_name = OsStr::new("patch");
        recovery
            .create(patch_name, patch.as_bytes())
            .map_err(|e| e.to_string())?;
        let patch_path = recovery.path.join(patch_name);
        let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
        if reverse {
            args.push("--reverse");
        }
        args.extend([
            "--",
            patch_path.to_str().ok_or(crate::i18n::text(
                "backend-non-utf8-private-patch-path-3e66d7",
            ))?,
        ]);
        transaction::alternate(repo, &candidate_path, &args)?;
        // Git creates the alternate index using core.sharedRepository and the actual
        // process umask. Do not substitute a hard-coded chmod for that policy.
        #[cfg(not(windows))]
        let file = recovery.read(candidate).map_err(|e| e.to_string())?;
        // FlushFileBuffers requires GENERIC_WRITE. Open the owned candidate before
        // copying its DACL; the retained handle also permits later publication.
        #[cfg(windows)]
        let file = recovery
            .read_writable(candidate)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let native_mode = file
            .metadata()
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777;
        if let Some(s) = &original {
            #[cfg(unix)]
            {
                let meta = file.metadata().map_err(|e| e.to_string())?;
                use std::os::unix::fs::MetadataExt;
                if meta.uid() != s.uid || meta.gid() != s.gid {
                    rustix::fs::fchown(
                        &file,
                        Some(rustix::process::Uid::from_raw(s.uid)),
                        Some(rustix::process::Gid::from_raw(s.gid)),
                    )
                    .map_err(|e| {
                        crate::i18n::message(
                            "backend-cannot-retain-index-ownership-e-70a7e8",
                            &[("detail", (e).to_string().into())],
                        )
                    })?;
                }
            }
            #[cfg(windows)]
            let _source_pin = parent.stable_source(name).map_err(|e| e.to_string())?;
            access::copy_index_policy(&file, &candidate_path, &s.policy, &parent.path.join(name))?;
            #[cfg(unix)]
            file.set_permissions(std::fs::Permissions::from_mode(native_mode))
                .map_err(|e| e.to_string())?;
            if access::read(&file, &candidate_path)? != s.policy {
                return Err(
                    crate::i18n::text("backend-configured-index-mode-conflicts-with-captured-acl-original-unchan-26e352").into(),
                );
            }
        }
        file.sync_all().map_err(|e| e.to_string())?;
        let prepared = Snapshot::from(file, &candidate_path)?;
        hook(Phase::Prepared);
        let current = Snapshot::read(&parent, name)?;
        if !same_optional(current.as_ref(), original.as_ref())
            || !lock.owned()
            || policy(repo)? != sharing
        {
            return Err(crate::i18n::text(
                "backend-index-identity-policy-or-lock-changed-before-publication-617d84",
            )
            .into());
        }
        transaction::verify(repo, lens)?;
        hook(Phase::BeforeMove);
        if original.is_some() {
            parent
                .move_file(name, &recovery, OsStr::new("previous"))
                .map_err(|e| e.to_string())?;
            hook(Phase::AfterMove);
            if !same_optional(
                Snapshot::read(&recovery, OsStr::new("previous"))?.as_ref(),
                original.as_ref(),
            ) || !parent.unchanged()
            {
                restore(&recovery, &parent, name);
                return Err(crate::i18n::text(
                    "backend-index-changed-during-preservation-captured-inode-retained-76824b",
                )
                .into());
            }
        }
        hook(Phase::BeforeInstall);
        if !lock.owned()
            || policy(repo)? != sharing
            || !same_optional(
                Snapshot::read(&recovery, OsStr::new("previous"))?.as_ref(),
                original.as_ref(),
            )
        {
            restore(&recovery, &parent, name);
            return Err(crate::i18n::text(
                "backend-index-policy-or-ownership-changed-before-installation-c48400",
            )
            .into());
        }
        if let Err(e) = recovery.link(candidate, &parent, name) {
            restore(&recovery, &parent, name);
            return Err(crate::i18n::message(
                "backend-index-create-only-installation-refused-e-f133bd",
                &[("detail", (e).to_string().into())],
            ));
        }
        hook(Phase::AfterInstall);
        if !lock.owned()
            || policy(repo)? != sharing
            || !Snapshot::read(&parent, name)?.is_some_and(|s| s.same(&prepared))
            || !same_optional(
                Snapshot::read(&recovery, OsStr::new("previous"))?.as_ref(),
                original.as_ref(),
            )
        {
            return Err(crate::i18n::text(
                "backend-index-changed-after-installation-previous-inode-retained-for-reco-4c9bd0",
            )
            .into());
        }
        hook(Phase::BeforeUnlock);
        lock.release_with(&mut || hook(Phase::DuringUnlock))?;
        if policy(repo)? != sharing
            || !Snapshot::read(&parent, name)?.is_some_and(|s| s.same(&prepared))
        {
            return Err(crate::i18n::text(
                "backend-index-changed-at-unlock-transaction-recovery-retained-82f660",
            )
            .into());
        }
        Ok(crate::i18n::message(
            "backend-staging-updated-previous-index-and-transaction-evidence-875f34",
            &[("path", (recovery.path.display()).to_string().into())],
        ))
    })();
    if result.is_err() {
        restore(&recovery, &parent, name);
    }
    result.map_err(|e| {
        crate::i18n::message(
            "backend-e-recovery-ce3c52",
            &[
                ("detail", (e).to_string().into()),
                ("path", (recovery.path.display()).to_string().into()),
            ],
        )
    })
}
fn same_optional(a: Option<&Snapshot>, b: Option<&Snapshot>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same(b),
        _ => false,
    }
}
fn restore(recovery: &Directory, parent: &Directory, name: &OsStr) {
    let _ = recovery.link(OsStr::new("previous"), parent, name);
}

#[cfg(all(test, unix))]
mod tests;
