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
        let file = recovery.read(candidate).map_err(|e| e.to_string())?;
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
mod tests {
    use super::*;
    use crate::git::file_comparison::{LensSpec, read_lens, tests::Repo};
    use std::os::unix::fs::PermissionsExt;
    fn record<T, E: std::fmt::Display>(result: Result<T, E>, error: &mut Option<String>) -> bool {
        match result {
            Ok(_) => true,
            Err(e) => {
                *error = Some(e.to_string());
                false
            }
        }
    }
    fn fixture() -> Result<(Repo, Lens, String), String> {
        let r = Repo::new()?;
        r.write("a", "before\n")?;
        r.write("unrelated", "unchanged\n")?;
        r.commit()?;
        r.write("a", "after\n")?;
        let lens = r.lens("a")?;
        let patch = String::from(
            "diff --git \"a/a\" \"b/a\"\n--- \"a/a\"\n+++ \"b/a\"\n@@ -1 +1 @@\n-before\n+after\n",
        );
        Ok((r, lens, patch))
    }
    fn bytes(path: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| e.to_string())
    }
    #[test]
    fn foreign_lock_regular_symlink_and_last_moment_replacements_are_preserved()
    -> Result<(), String> {
        for (phase, symlink) in [
            (Phase::Prepared, false),
            (Phase::Prepared, true),
            (Phase::BeforeInstall, false),
            (Phase::DuringUnlock, false),
        ] {
            let (r, lens, patch) = fixture()?;
            let idx = r.0.join(".git/index");
            let lock = r.0.join(".git/index.lock");
            let original = bytes(&idx)?;
            let outside = r.0.join("sentinel");
            std::fs::write(&outside, b"outside-original").map_err(|e| e.to_string())?;
            let displaced = r.0.join("owned-displaced-lock");
            let mut injected = false;
            let mut operation_hook_error: Option<String> = None;
            let result = apply(&r.0, &lens, &patch, false, &mut |at| {
                if at == phase {
                    if !record(
                        std::fs::rename(&lock, &displaced),
                        &mut operation_hook_error,
                    ) {
                        return;
                    }
                    if symlink {
                        if !record(
                            std::os::unix::fs::symlink(&outside, &lock),
                            &mut operation_hook_error,
                        ) {
                            return;
                        }
                    } else {
                        if !record(
                            std::fs::write(&lock, b"FOREIGN-LOCK-BYTES"),
                            &mut operation_hook_error,
                        ) {
                            return;
                        }
                    }
                    injected = true;
                }
            });
            if let Some(e) = operation_hook_error {
                return Err(e);
            }
            assert!(injected && result.is_err(), "{phase:?}: {result:?}");
            assert_eq!(bytes(&outside)?, b"outside-original");
            assert!(std::fs::symlink_metadata(&lock).is_ok());
            if !symlink {
                assert_eq!(bytes(&lock)?, b"FOREIGN-LOCK-BYTES");
            }
            assert_ne!(bytes(&idx)?, b"FOREIGN-LOCK-BYTES");
            if phase == Phase::Prepared {
                assert_eq!(bytes(&idx)?, original);
            }
            assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
            assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
        }
        Ok(())
    }
    #[test]
    fn swapped_index_parent_and_last_moment_index_creator_are_recoverable() -> Result<(), String> {
        for parent_swap in [true, false] {
            let (r, lens, patch) = fixture()?;
            let gitdir = r.0.join(".git");
            let saved = r.0.join("saved-git");
            let old = bytes(&gitdir.join("index"))?;
            let mut injected = false;
            let mut operation_hook_error: Option<String> = None;
            let result = apply(&r.0, &lens, &patch, false, &mut |at| {
                if parent_swap && at == Phase::BeforeMove {
                    if !record(std::fs::rename(&gitdir, &saved), &mut operation_hook_error) {
                        return;
                    }
                    if !record(std::fs::create_dir(&gitdir), &mut operation_hook_error) {
                        return;
                    }
                    if !record(
                        std::fs::write(gitdir.join("index"), b"FOREIGN-PARENT"),
                        &mut operation_hook_error,
                    ) {
                        return;
                    }
                    injected = true;
                } else if !parent_swap && at == Phase::BeforeInstall {
                    if !record(
                        std::fs::write(gitdir.join("index"), b"LATE-CREATOR"),
                        &mut operation_hook_error,
                    ) {
                        return;
                    }
                    injected = true;
                }
            });
            if let Some(e) = operation_hook_error {
                return Err(e);
            }
            assert!(injected && result.is_err());
            if parent_swap {
                assert_eq!(bytes(&gitdir.join("index"))?, b"FOREIGN-PARENT");
                assert_eq!(bytes(&saved.join("index"))?, old);
            } else {
                assert_eq!(bytes(&gitdir.join("index"))?, b"LATE-CREATOR");
                assert!(
                    std::fs::read_dir(&gitdir)
                        .map_err(|e| e.to_string())?
                        .filter_map(Result::ok)
                        .any(|e| bytes(&e.path().join("previous")).is_ok_and(|b| b == old))
                );
            }
        }
        Ok(())
    }
    #[test]
    fn configured_private_and_group_modes_match_native_git_and_policy_drift_is_refused()
    -> Result<(), String> {
        for sharing in ["0600", "group"] {
            let (r, _, patch) = fixture()?;
            let (native, _, _) = fixture()?;
            for repo in [&r, &native] {
                repo.git(&["config", "core.sharedRepository", sharing])?;
                repo.git(&["add", "a"])?;
            }
            let expected = std::fs::metadata(native.0.join(".git/index"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777;
            r.write("a", "next\n")?;
            let lens = r.lens("a")?;
            let patch = patch
                .replace("-before", "-after")
                .replace("+after", "+next");
            apply(&r.0, &lens, &patch, false, &mut |_| {})?;
            let mode = std::fs::metadata(r.0.join(".git/index"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, expected, "{sharing}");
            assert_eq!(r.git(&["show", ":a"])?, "next");
            assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
        }
        for config in [false, true] {
            let (r, lens, patch) = fixture()?;
            let idx = r.0.join(".git/index");
            let old = bytes(&idx)?;
            let mut operation_hook_error: Option<String> = None;
            let result = apply(&r.0, &lens, &patch, false, &mut |at| {
                if at == Phase::Prepared {
                    if config {
                        record(
                            r.git(&["config", "core.sharedRepository", "0600"]),
                            &mut operation_hook_error,
                        );
                    } else {
                        record(
                            std::fs::set_permissions(&idx, std::fs::Permissions::from_mode(0o600)),
                            &mut operation_hook_error,
                        );
                    }
                }
            });
            if let Some(e) = operation_hook_error {
                return Err(e);
            }
            assert!(result.is_err());
            assert_eq!(bytes(&idx)?, old);
        }
        let r = Repo::new()?;
        r.git(&["config", "core.sharedRepository", "0600"])?;
        r.write("a", "new\n")?;
        let lens = read_lens(
            &r.0,
            &LensSpec {
                path: "a".into(),
                previous: None,
            },
            &|| true,
        )?;
        apply(
            &r.0,
            &lens,
            "diff --git \"a/a\" \"b/a\"\nnew file mode 100644\n--- /dev/null\n+++ \"b/a\"\n@@ -0,0 +1 @@\n+new\n",
            false,
            &mut |_| {},
        )?;
        assert_eq!(
            std::fs::metadata(r.0.join(".git/index"))
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        Ok(())
    }
    #[test]
    fn split_index_keeps_unrelated_entries_and_reverses_through_the_new_publisher()
    -> Result<(), String> {
        let (r, _, patch) = fixture()?;
        r.git(&["update-index", "--split-index"])?;
        let lens = r.lens("a")?;
        apply(&r.0, &lens, &patch, false, &mut |_| {})?;
        assert_eq!(r.git(&["show", ":a"])?, "after");
        assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
        apply(&r.0, &r.lens("a")?, &patch, true, &mut |_| {})?;
        assert_eq!(r.git(&["show", ":a"])?, "before");
        assert_eq!(bytes(&r.0.join("a"))?, b"after\n");
        Ok(())
    }
    #[test]
    fn index_owner_comparison_and_real_group_drift_refuse_stale_publication() -> Result<(), String>
    {
        use std::os::unix::fs::MetadataExt;
        let (r, lens, patch) = fixture()?;
        let parent = Directory::open(&r.0.join(".git")).map_err(|e| e.to_string())?;
        let before = Snapshot::read(&parent, OsStr::new("index"))?.ok_or("index")?;
        let mut synthetic = Snapshot::read(&parent, OsStr::new("index"))?.ok_or("index")?;
        synthetic.uid = synthetic.uid.wrapping_add(1);
        assert!(!synthetic.same(&before));
        let idx = r.0.join(".git/index");
        let group = std::fs::metadata(&idx).map_err(|e| e.to_string())?.gid();
        let groups = rustix::process::getgroups().map_err(|e| e.to_string())?;
        if let Some(gid) = groups.iter().find(|g| g.as_raw() != group) {
            let mut hook_error = None;
            let result = apply(&r.0, &lens, &patch, false, &mut |phase| {
                if phase == Phase::Prepared
                    && let Err(e) = std::os::unix::fs::chown(&idx, None, Some(gid.as_raw()))
                {
                    hook_error = Some(e.to_string());
                }
            });
            if let Some(e) = hook_error {
                return Err(e);
            }
            assert!(result.is_err());
            assert_eq!(bytes(&idx)?, before.bytes);
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_index_acl_is_preserved_and_acl_only_drift_cannot_succeed() -> Result<(), String> {
        let (r, _, patch) = fixture()?;
        let idx = r.0.join(".git/index");
        let acl = vec![exacl::AclEntry::allow_user(
            "nobody",
            exacl::Perm::READ,
            None,
        )];
        exacl::setfacl(&[&idx], &acl, None).map_err(|e| e.to_string())?;
        let expected =
            exacl::getfacl(&idx, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
        apply(&r.0, &r.lens("a")?, &patch, false, &mut |_| {})?;
        assert_eq!(
            exacl::getfacl(&idx, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?,
            expected
        );
        r.write("a", "next\n")?;
        let lens = r.lens("a")?;
        let old = bytes(&idx)?;
        let changed = vec![exacl::AclEntry::deny_user(
            "nobody",
            exacl::Perm::READ,
            None,
        )];
        let mut operation_hook_error: Option<String> = None;
        let result = apply(
            &r.0,
            &lens,
            &patch
                .replace("-before", "-after")
                .replace("+after", "+next"),
            false,
            &mut |at| {
                if at == Phase::Prepared {
                    record(
                        exacl::setfacl(&[&idx], &changed, None),
                        &mut operation_hook_error,
                    );
                }
            },
        );
        if let Some(e) = operation_hook_error {
            return Err(e);
        }
        assert!(result.is_err());
        assert_eq!(bytes(&idx)?, old);
        Ok(())
    }
}
