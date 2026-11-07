//! Which process is the running instance: the one holding an exclusive lock on `instance.lock`
//! in the app's data folder. The system lets go of it when that process ends, crashed or not,
//! so a lock is never stale; and of two processes started at the same moment, only one gets it.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

const FILE: &str = "instance.lock";

/// The lock, held until dropped (or the process ends).
#[derive(Debug)]
pub struct Held {
    _file: File,
}

impl Drop for Held {
    fn drop(&mut self) {
        // Release before closing: a concurrently spawned child can briefly share the open
        // file description until exec, which otherwise keeps the lock alive after drop.
        let _ = self._file.unlock();
    }
}

/// What trying to take the lock found.
#[derive(Debug)]
pub enum Lock {
    /// This process is the running instance now.
    Held(Held),
    /// Another process is.
    Taken,
    /// There's no lock to take here (the folder can't be written, the file system can't lock).
    Unavailable(std::io::Error),
}

/// Tries to take the lock in `dir`, without waiting.
pub fn try_take(dir: &Path) -> Lock {
    let file = std::fs::create_dir_all(dir).and_then(|()| open(&dir.join(FILE)));
    let file = match file {
        Ok(file) => file,
        Err(e) => return Lock::Unavailable(e),
    };
    match file.try_lock() {
        Ok(()) => Lock::Held(Held { _file: file }),
        Err(TryLockError::WouldBlock) => Lock::Taken,
        Err(TryLockError::Error(e)) => Lock::Unavailable(e),
    }
}

fn open(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_holder_at_a_time_and_free_once_dropped() -> Result<(), String> {
        let dir = super::super::temp_dir("lock");
        let Lock::Held(first) = try_take(&dir) else {
            return Err("the first take failed".to_owned());
        };
        assert!(matches!(try_take(&dir), Lock::Taken));
        drop(first);
        assert!(matches!(try_take(&dir), Lock::Held(_)));
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
    }
}
