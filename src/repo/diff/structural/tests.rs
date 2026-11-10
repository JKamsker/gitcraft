use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::{Duration, Instant};

use super::{State, model, reader, tool};
use crate::git::{self, Area, StatusFile};
use crate::repo::{Action, DiffKey, DiffView, Msg, RepoTab};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-structural ü {}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Self(dir);
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.org"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            repo.git(&args)?;
        }
        Ok(repo)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.0), args)
    }
    fn write(&self, path: &str, contents: impl AsRef<[u8]>) -> Result<(), String> {
        std::fs::write(self.0.join(path), contents).map_err(|e| e.to_string())
    }
    fn commit(&self) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-qm", "change"])?;
        Ok(git::head(&self.0))
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn work(path: &str, area: Area, code: char) -> DiffKey {
    DiffKey::Work(StatusFile {
        path: path.to_owned(),
        old_path: None,
        code,
        area,
    })
}

// Sources; included to preserve qualified test names.
include!("tests/sources.rs");

// Tools; included to preserve qualified test names.
include!("tests/tools.rs");

// Lifecycle; included to preserve qualified test names.
include!("tests/lifecycle.rs");

// Supervision; included to preserve qualified test names.
include!("tests/supervision.rs");
