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
//! Some filesystems open the directory but do not implement its flush: SMB shares and WSL's
//! `\\wsl$` (9P) answer `ERROR_INVALID_FUNCTION` (observed). That is [`Flushed::Unsupported`],
//! an explicit outcome for the caller to note, never a silent success or a failure.
//!
//! Any other platform gets an `Unsupported` error, never a silent no-op: callers fail closed.
//!
//! This module is std-only: the recovery-recording harness (`src/resolver/park/checks`)
//! compiles it too.
use std::io;
use std::path::Path;
use std::time::Duration;

/// What flushing a directory did.
#[derive(Debug)]
pub(crate) enum Flushed {
    /// The filesystem accepted the flush.
    Done,
    /// The directory was opened, but its filesystem does not implement flushing a directory
    /// (Windows `ERROR_INVALID_FUNCTION`, `ERROR_NOT_SUPPORTED`, `ERROR_INVALID_PARAMETER` or
    /// `ERROR_CALL_NOT_IMPLEMENTED` from `FlushFileBuffers`). Nothing about durability is
    /// known: the caller records it and goes on, as nothing else could be done there.
    Unsupported(io::Error),
}

/// A directory flush that failed, and whether it failed opening the directory or flushing it.
#[derive(Debug)]
pub(crate) struct Failure {
    opened: bool,
    error: io::Error,
}

impl Failure {
    pub(crate) fn new(opened: bool, error: io::Error) -> Self {
        Self { opened, error }
    }

    /// Windows could not open the directory for flushing at all: this process may add
    /// neither files nor subdirectories there (`ERROR_ACCESS_DENIED` for both rights, so it
    /// cannot have created or renamed anything in it either), or another process holds it
    /// without `FILE_SHARE_WRITE` (`ERROR_SHARING_VIOLATION`). A flush that was opened and
    /// then refused is never this. Never on Unix, where every failure is a real failure.
    pub(crate) fn unopenable(&self) -> bool {
        !self.opened && (denied(&self.error) || sharing_violation(&self.error))
    }

    /// [`Self::unopenable`] because entries may not be added to it, not because it is held.
    pub(crate) fn denied(&self) -> bool {
        !self.opened && denied(&self.error)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stage = if self.opened { "flush" } else { "open" };
        write!(f, "{stage}: {}", self.error)
    }
}

/// Flushes `dir`'s entries. While another process holds `dir` without write sharing, the open
/// is retried a few times (about 0.2 s in all) before it fails.
pub(crate) fn sync(dir: &Path) -> Result<Flushed, Failure> {
    flush(retrying(|| open(dir), std::thread::sleep)?)
}

/// [`sync`] without retrying a held directory: for ancestors above the one being flushed, where
/// a held directory ends a climb instead of failing it.
pub(crate) fn sync_once(dir: &Path) -> Result<Flushed, Failure> {
    flush(open(dir).map_err(|e| Failure::new(false, e))?)
}

/// Pauses between attempts to open a directory another process holds without write sharing.
const BACKOFF_MS: [u64; 4] = [20, 40, 60, 80];

fn retrying(
    mut attempt: impl FnMut() -> io::Result<std::fs::File>,
    mut pause: impl FnMut(Duration),
) -> Result<std::fs::File, Failure> {
    for ms in BACKOFF_MS {
        match attempt() {
            Err(e) if sharing_violation(&e) => pause(Duration::from_millis(ms)),
            other => return other.map_err(|e| Failure::new(false, e)),
        }
    }
    attempt().map_err(|e| Failure::new(false, e))
}

fn flush(file: std::fs::File) -> Result<Flushed, Failure> {
    outcome(file.sync_all())
}

/// The outcome of an opened directory's flush. A refused flush (`PermissionDenied` from a
/// filter, say) stays a failure: only "not implemented here" is [`Flushed::Unsupported`].
fn outcome(flushed: io::Result<()>) -> Result<Flushed, Failure> {
    match flushed {
        Ok(()) => Ok(Flushed::Done),
        Err(e) if unsupported_flush(&e) => Ok(Flushed::Unsupported(e)),
        Err(e) => Err(Failure::new(true, e)),
    }
}

/// Windows codes a filesystem answers for a directory flush it does not implement.
fn unsupported_flush(error: &io::Error) -> bool {
    /// `ERROR_INVALID_FUNCTION`, `ERROR_NOT_SUPPORTED`, `ERROR_INVALID_PARAMETER`,
    /// `ERROR_CALL_NOT_IMPLEMENTED`.
    const UNSUPPORTED: [i32; 4] = [1, 50, 87, 120];
    cfg!(windows)
        && error
            .raw_os_error()
            .is_some_and(|code| UNSUPPORTED.contains(&code))
}

fn denied(error: &io::Error) -> bool {
    cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied
}

fn sharing_violation(error: &io::Error) -> bool {
    /// `ERROR_SHARING_VIOLATION`.
    const SHARING_VIOLATION: i32 = 32;
    cfg!(windows) && error.raw_os_error() == Some(SHARING_VIOLATION)
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

#[cfg(test)]
#[path = "dir_sync/outcome_tests.rs"]
mod outcome_tests;
