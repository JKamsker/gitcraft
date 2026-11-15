//! Shared read-only descriptor anchoring; no deletion operations.
use rustix::fd::OwnedFd;
use rustix::fs::{self, Mode, OFlags};
use std::path::{Component, Path};

const FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::CLOEXEC)
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW);

pub(in crate::git) fn open_root(path: &Path) -> Result<OwnedFd, String> {
    if !path.is_absolute() {
        return Err("Recovery root must be absolute".into());
    }
    let mut fd = fs::open("/", FLAGS, Mode::empty()).map_err(|e| e.to_string())?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                fd = fs::openat(&fd, name, FLAGS, Mode::empty()).map_err(|e| e.to_string())?
            }
            _ => return Err("Invalid recovery ancestry".into()),
        }
    }
    Ok(fd)
}

pub(in crate::git) fn child(
    parent: &OwnedFd,
    name: impl rustix::path::Arg,
) -> Result<OwnedFd, String> {
    #[cfg(target_os = "linux")]
    let fd = fs::openat2(
        parent,
        name,
        FLAGS,
        Mode::empty(),
        fs::ResolveFlags::BENEATH | fs::ResolveFlags::NO_SYMLINKS | fs::ResolveFlags::NO_XDEV,
    );
    #[cfg(not(target_os = "linux"))]
    let fd = fs::openat(parent, name, FLAGS, Mode::empty());
    let fd = fd.map_err(|e| format!("Purge directory boundary: {e}"))?;
    let parent_stat = fs::fstat(parent).map_err(|e| e.to_string())?;
    let stat = fs::fstat(&fd).map_err(|e| e.to_string())?;
    if parent_stat.st_dev != stat.st_dev {
        return Err("Device boundary".into());
    }
    Ok(fd)
}

/// Opening a regular entry also rejects file bind mounts, without reading its contents.
#[cfg(target_os = "linux")]
pub(in crate::git) fn inspect_entry(
    parent: &OwnedFd,
    name: impl rustix::path::Arg,
) -> Result<OwnedFd, String> {
    fs::openat2(
        parent,
        name,
        OFlags::PATH | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
        fs::ResolveFlags::BENEATH | fs::ResolveFlags::NO_SYMLINKS | fs::ResolveFlags::NO_XDEV,
    )
    .map_err(|e| e.to_string())
}
