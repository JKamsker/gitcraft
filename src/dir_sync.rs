//! Flushing a directory's own entries (names created, renamed or removed in it) to stable
//! storage, so that a new or renamed file stays reachable after a crash once this returns.
//!
//! Unix: `fsync` on a read-only descriptor of the directory.
//!
//! Windows: `FlushFileBuffers` on a directory handle, which std opens with
//! `FILE_FLAG_BACKUP_SEMANTICS`. The flush is refused (`ERROR_ACCESS_DENIED`) on a read-only
//! handle: the handle must carry `FILE_WRITE_DATA` or `FILE_APPEND_DATA`, which for a
//! directory are `FILE_ADD_FILE` and `FILE_ADD_SUBDIRECTORY`. Exactly one of them is
//! requested, never `GENERIC_WRITE`, so a directory that lets this process add only
//! subdirectories (a volume root for a standard user) can still be flushed. The handle shares
//! read, write and delete, so it never blocks another process renaming or removing entries.
//!
//! This module is std-only: the recovery-recording harness (`src/resolver/park/checks`)
//! compiles it too.
use std::io;
use std::path::Path;

/// Flushes `dir`'s entries. Every failure, including a filesystem that cannot flush a
/// directory, is returned: callers fail closed rather than assume durability.
pub(crate) fn sync(dir: &Path) -> io::Result<()> {
    open(dir)?.sync_all()
}

/// Whether `error` (from [`sync`]) means only that this process cannot open that directory
/// for flushing, on Windows: it may not add entries there at all (`ERROR_ACCESS_DENIED`, so
/// it cannot have created or renamed anything in it either), or another process holds the
/// directory open without `FILE_SHARE_WRITE` (`ERROR_SHARING_VIOLATION`, seen on
/// `%LOCALAPPDATA%`). Never on Unix: a directory is flushed through a read-only
/// descriptor there, and every error is a real failure.
pub(crate) fn unopenable_for_flush(error: &io::Error) -> bool {
    /// `ERROR_SHARING_VIOLATION`.
    const SHARING_VIOLATION: i32 = 32;
    cfg!(windows)
        && (error.kind() == io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(SHARING_VIOLATION))
}

#[cfg(unix)]
fn open(dir: &Path) -> io::Result<std::fs::File> {
    std::fs::File::open(dir)
}

#[cfg(windows)]
fn open(dir: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `FILE_FLAG_BACKUP_SEMANTICS`: open the directory itself.
    const DIRECTORY: u32 = 0x0200_0000;
    /// `FILE_ADD_FILE` (`FILE_WRITE_DATA`).
    const ADD_FILE: u32 = 0x0002;
    /// `FILE_ADD_SUBDIRECTORY` (`FILE_APPEND_DATA`).
    const ADD_SUBDIRECTORY: u32 = 0x0004;
    let with = |access: u32| {
        std::fs::OpenOptions::new()
            .access_mode(access)
            .custom_flags(DIRECTORY)
            .open(dir)
    };
    match with(ADD_FILE) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => with(ADD_SUBDIRECTORY),
        opened => opened,
    }
}

#[cfg(not(any(unix, windows)))]
fn open(_: &Path) -> io::Result<std::fs::File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory flush is not implemented on this platform",
    ))
}

#[cfg(test)]
#[path = "dir_sync/tests.rs"]
mod tests;
