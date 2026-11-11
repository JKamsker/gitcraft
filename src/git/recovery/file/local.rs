//! Local recovery destination ancestry and create-only file operations.

use super::*;

#[cfg(unix)]
pub(crate) fn parent(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
) -> Result<(std::fs::File, String), String> {
    use rustix::fs::{Mode, OFlags, fstat, open, openat};
    use std::os::unix::fs::MetadataExt;
    let parts: Vec<_> = path.split('/').collect();
    let (name, folders) = parts
        .split_last()
        .ok_or(crate::i18n::text("backend-empty-destination-93a1ec"))?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = std::fs::File::from(open(root, flags, Mode::empty()).map_err(|e| e.to_string())?);
    if native_token(&dir)? != pin.token {
        return Err(crate::i18n::text(
            "backend-the-working-root-filesystem-identity-changed-since-preview-1e3c36",
        )
        .into());
    }
    let metadata = std::fs::metadata(common).map_err(|e| e.to_string())?;
    let root_stat = fstat(&dir).map_err(|e| e.to_string())?;
    if root_stat.st_dev as u64 == metadata.dev() && root_stat.st_ino == metadata.ino() {
        return Err(crate::i18n::text(
            "backend-working-root-is-git-metadata-choose-another-repository-b744cc",
        )
        .into());
    }
    for folder in folders {
        dir = std::fs::File::from(openat(&dir, *folder, flags, Mode::empty()).map_err(|e| {
            crate::i18n::message(
                "backend-destination-parent-is-unavailable-or-linked-e-5f15ea",
                &[("detail", (e).to_string().into())],
            )
        })?);
        let stat = fstat(&dir).map_err(|e| e.to_string())?;
        if stat.st_dev as u64 == metadata.dev() && stat.st_ino == metadata.ino() {
            return Err(crate::i18n::text(
                "backend-recovery-cannot-create-files-inside-git-metadata-7c0c95",
            )
            .into());
        }
    }
    Ok((dir, (*name).into()))
}

#[cfg(unix)]
pub(super) fn local_available(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
) -> Result<(), String> {
    use rustix::fs::{AtFlags, statat};
    let (dir, name) = parent(root, common, path, pin)?;
    match statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err(crate::i18n::text(
            "backend-destination-already-exists-or-cannot-be-verified-7698f4",
        )
        .into()),
    }
}

#[cfg(unix)]
pub(super) fn local_create(
    root: &Path,
    common: &Path,
    path: &str,
    bytes: &[u8],
    pin: &RootPin,
) -> Result<(), String> {
    use rustix::fs::{Mode, OFlags, openat};
    let (dir, name) = parent(root, common, path, pin)?;
    let fd = openat(
        &dir,
        name.as_str(),
        OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::CREATE | OFlags::EXCL,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-exclusively-create-recovery-file-e-b44bcb",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            crate::i18n::message(
                "backend-a-new-recovery-file-may-be-incomplete-at-path-existing-files-were-01ba73",
                &[
                    ("path", (path).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })
}

#[cfg(windows)]
pub(super) fn physical_ancestors(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    if !root.is_absolute() {
        return Err(crate::i18n::text(
            "backend-recovery-root-must-be-a-physical-absolute-path-9c7a6c",
        )
        .into());
    }
    let mut paths: Vec<_> = root
        .ancestors()
        .filter(|path| path.has_root())
        .map(Path::to_path_buf)
        .collect();
    paths.reverse();
    paths.dedup();
    if paths.last().is_none_or(|path| path != root) {
        return Err(
            crate::i18n::text("backend-cannot-anchor-the-physical-recovery-path-8c4599").into(),
        );
    }
    Ok(paths)
}

#[cfg(windows)]
pub(crate) fn windows_parent(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
) -> Result<(Vec<std::fs::File>, std::path::PathBuf), String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let open = |path: &Path, no_follow: bool| {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | if no_follow { 0x0020_0000 } else { 0 })
            .open(path)
            .map_err(|e| e.to_string())
    };
    // Hold every physical component, not only the worktree and its descendants.
    // No-delete sharing directly prevents each ancestor's rename/replacement while
    // the final absolute-path CREATE_NEW is resolved. Reparse points fail closed.
    let mut held = Vec::new();
    for ancestor in physical_ancestors(root)? {
        let directory = open(&ancestor, true)?;
        let meta = directory.metadata().map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(crate::i18n::text(
                "backend-a-physical-recovery-ancestor-is-linked-or-unavailable-4d9ba0",
            )
            .into());
        }
        held.push(directory);
    }
    let root_file = held.last().ok_or(crate::i18n::text(
        "backend-missing-physical-recovery-root-9c47ec",
    ))?;
    if native_token(root_file)? != pin.token {
        return Err(crate::i18n::text(
            "backend-the-working-root-filesystem-identity-changed-since-preview-1e3c36",
        )
        .into());
    }
    let metadata_root = std::fs::canonicalize(common).map_err(|e| e.to_string())?;
    if std::fs::canonicalize(root).map_err(|e| e.to_string())? == metadata_root {
        return Err(crate::i18n::text("backend-working-root-is-git-metadata-6377ab").into());
    }
    let mut dest = root.to_path_buf();
    let parts: Vec<_> = path.split('/').collect();
    let (name, folders) = parts
        .split_last()
        .ok_or(crate::i18n::text("backend-empty-destination-93a1ec"))?;
    for folder in folders {
        dest.push(folder);
        let dir = open(&dest, true)?;
        let meta = dir.metadata().map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(crate::i18n::text(
                "backend-destination-parent-is-linked-or-not-a-folder-143350",
            )
            .into());
        }
        if std::fs::canonicalize(&dest).map_err(|e| e.to_string())? == metadata_root {
            return Err(
                crate::i18n::text("backend-destination-crosses-git-metadata-e5ce00").into(),
            );
        }
        held.push(dir);
    }
    dest.push(name);
    Ok((held, dest))
}

#[cfg(windows)]
pub(super) fn local_available(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
) -> Result<(), String> {
    let (_held, dest) = windows_parent(root, common, path, pin)?;
    match std::fs::symlink_metadata(dest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(crate::i18n::text(
            "backend-destination-already-exists-or-cannot-be-verified-7698f4",
        )
        .into()),
    }
}

#[cfg(windows)]
pub(super) fn local_create(
    root: &Path,
    common: &Path,
    path: &str,
    bytes: &[u8],
    pin: &RootPin,
) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    let (_held, dest) = windows_parent(root, common, path, pin)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(3)
        .custom_flags(0x0020_0000)
        .open(dest)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            crate::i18n::message(
                "backend-a-new-recovery-file-may-be-incomplete-at-path-existing-files-were-01ba73",
                &[
                    ("path", (path).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn local_available(_: &Path, _: &Path, _: &str, _: &RootPin) -> Result<(), String> {
    Err(
        crate::i18n::text("backend-safe-new-file-recovery-is-unsupported-on-this-platform-c88192")
            .into(),
    )
}

#[cfg(not(any(unix, windows)))]
pub(super) fn local_create(
    _: &Path,
    _: &Path,
    _: &str,
    _: &[u8],
    _: &RootPin,
) -> Result<(), String> {
    Err(
        crate::i18n::text("backend-safe-new-file-recovery-is-unsupported-on-this-platform-c88192")
            .into(),
    )
}
