//! Directory flushes for receipts, archives and live restores (see `crate::dir_sync`).
use super::*;
use crate::dir_sync::{Failure, Flushed};

/// Flushes `dir`'s entries on every platform: Windows too (`FlushFileBuffers` on a
/// directory handle). Renames do not request write-through; a rename is durable once the
/// directories it touched are flushed by this. A held directory is retried briefly; a
/// filesystem without directory flushes (SMB, `\\wsl$`) is noted, not failed.
pub(in crate::resolver::park) fn sync_dir(dir: &Path) -> Result<(), String> {
    match crate::dir_sync::sync(dir) {
        Ok(flushed) => {
            unsupported(dir, &flushed);
            Ok(())
        }
        Err(failure) => Err(format!("sync {}: {failure}", dir.display())),
    }
}

/// Persist reachability, including ancestors created by repo_file/reserve.
pub(in crate::resolver::park) fn sync_ancestors(path: &Path) -> Result<(), String> {
    climb_ancestors(path, &mut Default::default(), false, flush)
}

/// [`sync_ancestors`] of each live directory (children first), flushing a shared ancestor
/// once rather than once per file below it (each flush is a real `FlushFileBuffers` call on
/// Windows). Unlike an archive directory, a live directory may be one this process may not
/// add entries to (a deny ACL on a tracked folder): then neither this restore nor Git running
/// as this process created or renamed anything in it, so on Windows it is not flushed and the
/// climb goes on to its parent. A live directory held by another process without write
/// sharing (after the retries) still fails the restore.
pub(in crate::resolver::park) fn sync_ancestors_of<'a>(
    dirs: impl IntoIterator<Item = &'a Path>,
) -> Result<(), String> {
    sync_each(dirs, flush)
}

/// The production flush: patient (retrying a held directory) for the directory requested,
/// a single attempt for the ancestors above it.
fn flush(dir: &Path, requested: bool) -> Result<Flushed, Failure> {
    if requested {
        crate::dir_sync::sync(dir)
    } else {
        crate::dir_sync::sync_once(dir)
    }
}

/// A directory whose filesystem cannot flush directories leaves a diagnostic note (kept by
/// the test and recording harnesses) and is otherwise treated as flushed: nothing more can
/// be done there, and failing would make parking impossible on such filesystems.
fn unsupported(dir: &Path, flushed: &Flushed) {
    if let Flushed::Unsupported(e) = flushed {
        super::super::effects::note(&format!(
            "directory-flush-unsupported {}: {e}",
            dir.display()
        ));
    }
}

fn sync_each<'a>(
    dirs: impl IntoIterator<Item = &'a Path>,
    mut sync: impl FnMut(&Path, bool) -> Result<Flushed, Failure>,
) -> Result<(), String> {
    let mut done = std::collections::BTreeSet::new();
    for dir in dirs {
        climb_ancestors(dir, &mut done, true, &mut sync)?;
    }
    Ok(())
}

/// `path` itself must flush, unless `live` and this process may not add entries to it (see
/// [`sync_ancestors_of`]). On Windows the climb stops at the first ancestor that cannot be
/// opened for flushing: one this process may not add entries to (e.g. `C:\Users` for a
/// standard user), which it cannot have created anything in, or one another process holds
/// without write sharing (e.g. `%LOCALAPPDATA%`). Above the archive store such ancestors
/// predate gitcraft. Inside it, a directory gitcraft created is retried by every later
/// receipt's climb, but stays unflushed for as long as another process keeps holding it.
/// A flush that was opened and then refused always fails. Unix flushes through read-only
/// descriptors and propagates every failure. A directory in `done` was flushed (or noted as
/// unflushable) already, and the climb from it already ran.
fn climb_ancestors(
    path: &Path,
    done: &mut std::collections::BTreeSet<PathBuf>,
    live: bool,
    mut sync: impl FnMut(&Path, bool) -> Result<Flushed, Failure>,
) -> Result<(), String> {
    let mut at = Some(path);
    while let Some(dir) = at {
        if done.contains(dir) {
            break;
        }
        let requested = dir == path;
        match sync(dir, requested) {
            Ok(flushed) => unsupported(dir, &flushed),
            // Not remembered: should it be another call's own `path`, it must flush then.
            Err(failure) if !requested && failure.unopenable() => break,
            Err(failure) if requested && live && failure.denied() => {
                at = dir.parent();
                continue;
            }
            Err(failure) => return Err(format!("sync {}: {failure}", dir.display())),
        }
        done.insert(dir.to_path_buf());
        super::super::effects::boundary("directory-synced")?;
        at = dir.parent();
    }
    Ok(())
}

#[cfg(test)]
#[path = "ancestor_tests.rs"]
mod ancestor_tests;

#[cfg(test)]
#[path = "live_dir_tests.rs"]
mod live_dir_tests;
