//! Extracted from reviewed fileviews 1535d831; serde added for durable shelf/undo policy.
//! Exact bounded access-policy snapshots, independent of rename-sensitive ctime.
#[cfg(unix)]
use std::{fs::File, path::Path};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccessMetadata {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    #[cfg(target_os = "macos")]
    acl: Vec<exacl::AclEntry>,
    #[cfg(windows)]
    descriptor: Vec<u8>,
}

#[cfg(unix)]
pub(crate) fn read(file: &File, path: &Path) -> Result<AccessMetadata, String> {
    #[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
    {
        #[cfg(not(target_os = "macos"))]
        let _ = path;
        use rustix::fs::{fgetxattr, fgetxattr_query, flistxattr, flistxattr_query};
        const LIMIT: usize = 1024 * 1024;
        let size = flistxattr_query(file).map_err(|e| {
            crate::i18n::message(
                "backend-cannot-snapshot-output-access-metadata-e-94a33b",
                &[("detail", (e).to_string().into())],
            )
        })?;
        if size > LIMIT {
            return Err(
                crate::i18n::text("backend-output-access-metadata-exceeds-1-mib-9bd2af").into(),
            );
        }
        let mut names = vec![0; size];
        let n = flistxattr(file, &mut names[..]).map_err(|e| e.to_string())?;
        if n > names.len() {
            return Err(
                crate::i18n::text("backend-output-metadata-changed-while-listing-aef402").into(),
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
                return Err(crate::i18n::text(
                    "backend-output-access-metadata-exceeds-1-mib-9bd2af",
                )
                .into());
            }
            let mut value = vec![0; size];
            let n = fgetxattr(file, name, &mut value[..]).map_err(|e| e.to_string())?;
            if n > value.len() {
                return Err(crate::i18n::text(
                    "backend-output-metadata-changed-while-reading-d70336",
                )
                .into());
            }
            value.truncate(n);
            total = next.ok_or(crate::i18n::text("backend-output-metadata-overflow-678173"))?;
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
        {
            let _ = (file, path);
            Err(crate::i18n::text(
                "backend-native-working-state-access-policy-is-unavailable-on-windows-22cb83",
            )
            .into())
        }
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
            return Err(crate::i18n::text(
                "backend-output-identity-changed-during-native-acl-read-624d92",
            )
            .into());
        }
        Ok(())
    };
    check()?;
    let acl = exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL).map_err(|e| e.to_string())?;
    if acl.len() > 128 || acl.iter().any(|e| e.name.len() > 1024) {
        return Err(
            crate::i18n::text("backend-native-acl-exceeds-bounded-snapshot-limits-52b958").into(),
        );
    }
    check()?;
    Ok(acl)
}

/// Restore a captured index policy only on an owned candidate, then verify the complete
/// metadata. Failure leaves the original index in place.
#[cfg(unix)]
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
            return Err(crate::i18n::text(
                "backend-index-access-metadata-could-not-be-preserved-3b9ff1",
            )
            .into());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let _ = (file, path, original);
        Err(crate::i18n::text(
            "backend-native-working-state-access-policy-is-unavailable-on-windows-22cb83",
        )
        .into())
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        windows
    )))]
    {
        let _ = (file, path, original);
        Err(
            crate::i18n::text("backend-index-access-policy-is-unsupported-on-this-platform-5b2f02")
                .into(),
        )
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
            "backend-could-not-preserve-output-acl-e-6734a5",
            &[("detail", (e).to_string().into())],
        )
    })?;
    if mac_acl(file, path)? != original.acl {
        return Err(crate::i18n::text(
            "backend-replacement-acl-differs-from-the-captured-output-policy-47d11f",
        )
        .into());
    }
    Ok(())
}
