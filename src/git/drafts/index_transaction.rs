//! Narrow Git-internal adapter for the one retained-directory/full-policy publisher.
//! Lists and repository_tools prepare bytes privately; neither publishes via index.lock.
use super::native_files::{Directory, Replacement, Snapshot};
use std::{ffi::OsStr, path::Path, sync::Arc};
#[derive(Clone, Debug)]
pub(in crate::git) struct IndexSnapshot {
    directory: Arc<Directory>,
    name: String,
    snapshot: Arc<Snapshot>,
}
impl PartialEq for IndexSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.directory.same(&other.directory)
            && self.directory.path == other.directory.path
            && self.name == other.name
            && self.snapshot == other.snapshot
    }
}
impl Eq for IndexSnapshot {}
impl IndexSnapshot {
    pub(in crate::git) fn capture(path: &Path) -> Result<Option<Self>, String> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(crate::i18n::text(
                "backend-native-index-publication-is-unavailable-on-this-platform-3fda01",
            )
            .into());
        }
        let parent = path
            .parent()
            .ok_or(crate::i18n::text("backend-missing-index-parent-f550c8"))?;
        if std::fs::canonicalize(parent).map_err(|e| e.to_string())? != parent {
            return Err(crate::i18n::text(
                "backend-index-parent-must-be-a-physical-native-directory-f351f4",
            )
            .into());
        }
        let directory = Arc::new(Directory::open(parent).map_err(|e| e.to_string())?);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(crate::i18n::text(
                "backend-index-basename-must-be-utf-8-45a39e",
            ))?
            .to_owned();
        Ok(
            Snapshot::read(&directory, &name, 32 * 1024 * 1024)?.map(|snapshot| Self {
                directory,
                name,
                snapshot: Arc::new(snapshot),
            }),
        )
    }
    pub(in crate::git) fn bytes(&self) -> &[u8] {
        &self.snapshot.bytes
    }
    pub(in crate::git) fn unchanged(&self) -> bool {
        self.directory.unchanged() && self.snapshot.same_at(&self.directory, &self.name)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LockDisposition {
    CapturedOwned,
    ReturnCaptured,
}
fn lock_disposition(identity: bool, bytes: bool, policy: bool) -> LockDisposition {
    if identity && bytes && policy {
        LockDisposition::CapturedOwned
    } else {
        LockDisposition::ReturnCaptured
    }
}
pub(in crate::git) struct IndexTransaction {
    replacement: Replacement,
    lock_name: String,
    lock: Snapshot,
    finished: bool,
}
impl IndexTransaction {
    /// Captures original index and installs privately prepared bytes create-only.
    /// On error, every moved foreign entry is returned create-only or retained with
    /// its exact evidence path. This never executes Git or follows index.lock bytes.
    pub(in crate::git) fn new(before: &IndexSnapshot, after: Vec<u8>) -> Result<Self, String> {
        if after.len() > 32 * 1024 * 1024 {
            return Err(crate::i18n::text("backend-prepared-index-exceeds-32-mib-7b2bd0").into());
        }
        if !before.unchanged() {
            return Err(crate::i18n::text(
                "backend-index-or-retained-parent-changed-before-publication-2f4e23",
            )
            .into());
        }
        let observed = Snapshot::read(&before.directory, &before.name, 32 * 1024 * 1024)?
            .ok_or(crate::i18n::text("backend-index-disappeared-2b459a"))?;
        if &observed != before.snapshot.as_ref() {
            return Err(crate::i18n::text(
                "backend-index-bytes-identity-full-policy-changed-7f1ef7",
            )
            .into());
        }
        let policy = observed.policy.clone();
        let replacement = Replacement::prepare(
            before.directory.clone(),
            before.name.clone(),
            Some(observed),
            Some(&after),
            Some(&policy),
            None,
        )?;
        let lock_name = format!("{}.lock", before.name);
        let file = replacement
            .recovery
            .create(OsStr::new("index-lock"), b"")
            .map_err(|e| e.to_string())?;
        let created = same_file::Handle::from_file(file).map_err(|e| e.to_string())?;
        let lock = Snapshot::read(&replacement.recovery, "index-lock", 0)?
            .ok_or(crate::i18n::text("backend-private-lock-disappeared-a23815"))?;
        let private = same_file::Handle::from_file(
            replacement
                .recovery
                .read(OsStr::new("index-lock"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        if private != created {
            return Err(crate::i18n::message(
                "backend-private-lock-changed-retained-e393a1",
                &[("replacement", (replacement.evidence()).to_string().into())],
            ));
        }
        replacement
            .recovery
            .link(
                OsStr::new("index-lock"),
                &replacement.parent,
                OsStr::new(&lock_name),
            )
            .map_err(|e| {
                crate::i18n::message("backend-index-is-locked-or-create-only-lock-is-unavailable-e-private-evid-46d356", &[("detail", (e).to_string().into()), ("replacement", (replacement.evidence()).to_string().into())])
            })?;
        let mut transaction = Self {
            replacement,
            lock_name,
            lock,
            finished: false,
        };
        let published: Result<(), String> = (|| {
            if !transaction
                .lock
                .same_at(&transaction.replacement.parent, &transaction.lock_name)
            {
                return Err(crate::i18n::text(
                    "backend-index-lock-identity-bytes-policy-changed-before-publication-0ed5ca",
                )
                .into());
            }
            transaction.replacement.publish()?;
            if !transaction
                .lock
                .same_at(&transaction.replacement.parent, &transaction.lock_name)
            {
                return Err(crate::i18n::text(
                    "backend-index-lock-changed-during-publication-fec309",
                )
                .into());
            }
            Ok(())
        })();
        if let Err(error) = published {
            let rollback = transaction.abort();
            return Err(crate::i18n::message(
                "backend-error-abort-receipt-rollback-f875d0",
                &[
                    ("detail", (error).to_string().into()),
                    ("rollback", format!("{:?}", rollback).into()),
                ],
            ));
        }
        Ok(transaction)
    }
    fn release_lock(&self) -> Result<(), String> {
        let r = &self.replacement;
        if !r.parent.unchanged() || !r.recovery.unchanged() {
            return Err(crate::i18n::message(
                "backend-lock-parent-moved-evidence-retained-in-held-directory-formerly-0c41fb",
                &[("r", (r.evidence()).to_string().into())],
            ));
        }
        // Capture first: no check-then-unlink of the public lock pathname.
        r.parent
            .move_file(
                OsStr::new(&self.lock_name),
                &r.recovery,
                OsStr::new("released-lock"),
            )
            .map_err(|e| {
                crate::i18n::message(
                    "backend-lock-release-capture-failed-e-evidence-0c66f8",
                    &[
                        ("detail", (e).to_string().into()),
                        ("r", (r.evidence()).to_string().into()),
                    ],
                )
            })?;
        let observed = Snapshot::read(&r.recovery, "released-lock", 32 * 1024 * 1024);
        let disposition = match observed {
            Ok(Some(ref observed)) => lock_disposition(
                observed == &self.lock,
                observed.bytes == self.lock.bytes,
                observed.policy == self.lock.policy,
            ),
            _ => LockDisposition::ReturnCaptured,
        };
        if disposition == LockDisposition::CapturedOwned {
            return Ok(());
        }
        let restored = r.recovery.link(
            OsStr::new("released-lock"),
            &r.parent,
            OsStr::new(&self.lock_name),
        );
        Err(crate::i18n::message(
            "backend-a-foreign-changed-lock-was-captured-and-preserved-create-only-ret-59caa2",
            &[
                ("restored", format!("{:?}", restored).into()),
                ("r", (r.evidence()).to_string().into()),
            ],
        ))
    }
    pub(in crate::git) fn accept(&mut self) -> Result<String, String> {
        if self.finished {
            return Err(
                crate::i18n::text("backend-index-transaction-already-resolved-f3e97e").into(),
            );
        }
        self.finished = true;
        self.replacement.verify_installed()?;
        self.release_lock()?;
        Ok(self.replacement.evidence())
    }
    pub(in crate::git) fn abort(&mut self) -> Result<String, String> {
        if self.finished {
            return Err(crate::i18n::text(
                "backend-index-transaction-already-resolved-no-rollback-attempted-db597e",
            )
            .into());
        }
        self.finished = true;
        let rollback = self.replacement.rollback();
        let lock = self.release_lock();
        if rollback.is_err() || lock.is_err() {
            return Err(crate::i18n::message(
                "backend-index-rollback-rollback-lock-release-lock-evidence-242d6f",
                &[
                    ("rollback", format!("{:?}", rollback).into()),
                    ("lock", format!("{:?}", lock).into()),
                    (
                        "replacement",
                        (self.replacement.evidence()).to_string().into(),
                    ),
                ],
            ));
        }
        Ok(self.replacement.evidence())
    }
    pub(in crate::git) fn evidence(&self) -> String {
        self.replacement.evidence()
    }
    pub(in crate::git) fn retain(&mut self) -> String {
        self.finished = true;
        crate::i18n::message(
            "backend-index-lock-and-all-original-candidate-entries-retained-at-393f61",
            &[(
                "replacement",
                (self.replacement.evidence()).to_string().into(),
            )],
        )
    }
}
// There is no Drop rollback or public-path cleanup. A caller with a lost receipt
// must retain; a caller proving no ref publication must explicitly abort.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_index_lock_capture_requires_identity_bytes_and_full_policy() {
        assert_eq!(
            lock_disposition(true, true, true),
            LockDisposition::CapturedOwned
        );
        for (identity, bytes, policy) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert_eq!(
                lock_disposition(identity, bytes, policy),
                LockDisposition::ReturnCaptured
            );
        }
    }
}
