//! Exact bounded access-policy snapshots, independent of rename-sensitive ctime.
use std::{fs::File, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessMetadata {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    #[cfg(target_os = "macos")]
    acl: Vec<exacl::AclEntry>,
    #[cfg(windows)]
    descriptor: Vec<u8>,
}
impl AccessMetadata {
    pub fn replacement_matches(&self, original: &Self) -> bool {
        #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
        {
            fn retained(name: &[u8]) -> bool {
                !matches!(
                    name,
                    b"security.capability" | b"security.ima" | b"security.evm"
                )
            }
            #[cfg(target_os = "macos")]
            if self.acl != original.acl {
                return false;
            }
            self.entries.iter().all(|(name, _)| retained(name))
                && self
                    .entries
                    .iter()
                    .eq(original.entries.iter().filter(|(name, _)| retained(name)))
        }
        #[cfg(windows)]
        {
            self == original
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            windows
        )))]
        {
            let _ = original;
            false
        }
    }
}

pub(crate) fn read(file: &File, path: &Path) -> Result<AccessMetadata, String> {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    {
        #[cfg(not(target_os = "macos"))]
        let _ = path;
        use rustix::fs::{fgetxattr, fgetxattr_query, flistxattr, flistxattr_query};
        const LIMIT: usize = 1024 * 1024;
        let size = flistxattr_query(file).map_err(|e| {
            crate::i18n::message(
                "interaction-standalone-error-access-snapshot",
                &[("detail", (e.to_string()).into())],
            )
        })?;
        if size > LIMIT {
            return Err(crate::i18n::text("interaction-standalone-error-access-limit").into());
        }
        let mut names = vec![0; size];
        let n = flistxattr(file, &mut names[..]).map_err(|e| e.to_string())?;
        if n > names.len() {
            return Err(
                crate::i18n::text("interaction-standalone-error-metadata-list-changed").into(),
            );
        }
        names.truncate(n);
        let mut names = names
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        names.sort_unstable();
        let mut exact = Vec::new();
        let mut total = 0usize;
        for name in names {
            let size = fgetxattr_query(file, name).map_err(|e| e.to_string())?;
            let next = total
                .checked_add(name.len())
                .and_then(|n| n.checked_add(size))
                .and_then(|n| n.checked_add(16));
            if next.is_none_or(|n| n > LIMIT) {
                return Err(crate::i18n::text("interaction-standalone-error-access-limit").into());
            }
            let mut value = vec![0; size];
            let n = fgetxattr(file, name, &mut value[..]).map_err(|e| e.to_string())?;
            if n > value.len() {
                return Err(crate::i18n::text(
                    "interaction-standalone-error-metadata-read-changed",
                )
                .into());
            }
            value.truncate(n);
            total = next.ok_or(crate::i18n::text(
                "interaction-standalone-error-metadata-overflow",
            ))?;
            exact.push((name.to_vec(), value));
        }
        #[cfg(target_os = "macos")]
        let acl = mac_acl(file, path)?;
        Ok(AccessMetadata {
            entries: exact,
            #[cfg(target_os = "macos")]
            acl,
        })
    }
    #[cfg(windows)]
    {
        let _ = file;
        super::windows_metadata::snapshot(path).map(|descriptor| AccessMetadata { descriptor })
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        windows
    )))]
    {
        let _ = (file, path);
        // No unsafe or text-parsed native ACL API fallback. Viewing stays available;
        // existing-output saves explicitly refuse on these platforms.
        Ok(AccessMetadata {})
    }
}

#[cfg(target_os = "macos")]
fn mac_acl(file: &File, path: &Path) -> Result<Vec<exacl::AclEntry>, String> {
    // The safe ACL API is path-based and explicitly no-follow. Verify that path
    // still names the held file on both sides; callers sample the whole policy
    // twice and also retain raw bytes/identity/metadata.
    let identity = same_file::Handle::from_file(file.try_clone().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let check = || -> Result<(), String> {
        use rustix::fs::{Mode, OFlags, open};
        let current = File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?,
        );
        let current = same_file::Handle::from_file(current).map_err(|e| e.to_string())?;
        if current != identity {
            return Err(
                crate::i18n::text("interaction-standalone-error-acl-identity-changed").into(),
            );
        }
        Ok(())
    };
    check()?;
    let acl = exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
    if acl.len() > 128 || acl.iter().any(|e| e.name.len() > 1024) {
        return Err(crate::i18n::text("interaction-standalone-error-acl-limit").into());
    }
    check()?;
    Ok(acl)
}

/// Restore a captured index policy only on an owned candidate, then verify the complete
/// metadata. Failure leaves the original index in place.
pub(crate) fn copy_index_policy(
    file: &File,
    path: &Path,
    original: &AccessMetadata,
    source: &Path,
) -> Result<(), String> {
    let _ = source;
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    {
        for (name, bytes) in &original.entries {
            rustix::fs::fsetxattr(
                file,
                name.as_slice(),
                bytes,
                rustix::fs::XattrFlags::empty(),
            )
            .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "macos")]
        restore_mac_acl(file, path, original)?;
        if read(file, path)? != *original {
            return Err(
                crate::i18n::text("interaction-standalone-error-index-metadata-preserve").into(),
            );
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        super::windows_metadata::prepare(source, path)?;
        if read(file, path)? != *original {
            return Err(
                crate::i18n::text("interaction-standalone-error-index-policy-preserve").into(),
            );
        }
        Ok(())
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        windows
    )))]
    {
        let _ = (file, path, original);
        Err(crate::i18n::text("interaction-standalone-error-index-policy-unsupported").into())
    }
}

/// APFS clonefile inherits destination ACLs by default. Install the captured source ACL
/// exactly on the owned replacement, including an empty ACL, then verify its identity and
/// policy. This never sets metadata on the original destination.
#[cfg(target_os = "macos")]
pub(super) fn restore_mac_acl(
    file: &File,
    path: &Path,
    original: &AccessMetadata,
) -> Result<(), String> {
    // mac_acl verifies that this path still names the held, no-follow regular file.
    let _ = mac_acl(file, path)?;
    exacl::setfacl(&[path], &original.acl, exacl::AclOption::SYMLINK_ACL).map_err(|e| {
        crate::i18n::message(
            "interaction-standalone-error-output-acl",
            &[("detail", (e.to_string()).into())],
        )
    })?;
    if mac_acl(file, path)? != original.acl {
        return Err(crate::i18n::text("interaction-standalone-error-replacement-acl").into());
    }
    Ok(())
}
