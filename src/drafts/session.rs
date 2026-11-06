//! Whether the last run ended cleanly. Each running gitcraft keeps a marker file in the
//! `sessions` folder, locked for as long as it runs, and removes it on a normal close. The
//! system lets go of the lock when a process ends however it ends, so a marker found that can
//! be locked is one a crash left: the run it belongs to is gone without closing. One still
//! locked is another gitcraft running now, and is left alone.

use std::fs::{self, File, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// This run's marker.
pub(crate) struct Session {
    file: Option<(PathBuf, File)>,
    /// A run before this one ended without closing (until said).
    pub unclean: bool,
}

impl Session {
    /// Starts this run's marker in gitcraft's data folder, and looks for one a crash left.
    pub(crate) fn start() -> Self {
        match super::data_dir() {
            Some(root) => Self::start_in(&folder(&root)),
            None => Self {
                file: None,
                unclean: false,
            },
        }
    }

    /// [`Self::start`] with the markers in `folder`.
    pub(super) fn start_in(folder: &Path) -> Self {
        let unclean = clear_left_behind(folder);
        Self {
            file: create(folder),
            unclean,
        }
    }

    /// A normal close: the marker goes, so the next start doesn't take this run for a crash.
    pub(crate) fn end(&mut self) {
        if let Some((path, file)) = self.file.take() {
            drop(file);
            let _ = fs::remove_file(path);
        }
    }
}

fn folder(root: &Path) -> PathBuf {
    root.join("sessions")
}

/// Removes the markers of runs that ended without closing; whether there were any.
fn clear_left_behind(folder: &Path) -> bool {
    let Ok(list) = fs::read_dir(folder) else {
        return false;
    };
    let mut found = false;
    for entry in list.flatten() {
        let path = entry.path();
        // A `.starting` one is a marker being made, or one a crash interrupted while it was.
        let marker = match path.extension().and_then(|e| e.to_str()) {
            Some("lock") => true,
            Some("starting") => false,
            _ => continue,
        };
        let Ok(file) = File::options().read(true).write(true).open(&path) else {
            continue;
        };
        match file.try_lock() {
            Ok(()) => {
                found |= marker;
                drop(file);
                let _ = fs::remove_file(&path);
            }
            // Another gitcraft running now holds it.
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(_)) => {}
        }
    }
    found
}

/// This run's marker, locked until the process ends. `None` when it can't be made: crashes
/// then go unnoticed, which is all that is lost. It is made and locked under a name other
/// runs don't look at, and only then named as a marker: another gitcraft starting at the same
/// moment never finds it unlocked and takes it for a crash.
fn create(folder: &Path) -> Option<(PathBuf, File)> {
    fs::create_dir_all(folder).ok()?;
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = format!("{}-{started}", std::process::id());
    let starting = folder.join(format!("{name}.starting"));
    let path = folder.join(format!("{name}.lock"));
    let mut file = File::create(&starting).ok()?;
    if file.try_lock().is_err() {
        let _ = fs::remove_file(&starting);
        return None;
    }
    let _ = writeln!(file, "gitcraft {} running", std::process::id());
    if fs::rename(&starting, &path).is_err() {
        drop(file);
        let _ = fs::remove_file(&starting);
        return None;
    }
    Some((path, file))
}
