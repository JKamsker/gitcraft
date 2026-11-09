//! The last read of a repository's branches and newest history, kept on disk: on opening it
//! again (gitcraft starting with it open) the graph shows at once, as it was, while git reads
//! it anew. Only shown until that read is in, and nothing is done from it before (whatever
//! waits for a first read, `history_gen > 0`, still waits).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::RepoTab;
use crate::git::{Commit, Refs};

/// What a snapshot is in, changed when it is read differently.
const VERSION: u32 = 1;
/// The newest commits kept: more than a window shows, few enough to read in an instant.
const COMMITS: usize = 400;
/// A snapshot is written again after a read at most this often.
const EVERY: Duration = Duration::from_secs(120);
/// Larger snapshots are left unread (something else wrote it).
const MAX_BYTES: u64 = 8 << 20;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    refs: Refs,
    history: Vec<Commit>,
}

/// The file of the repository (or worktree) at `repo`, named by a hash of its path. None in
/// tests: they neither read the user's nor leave theirs.
fn file(repo: &Path) -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    let digest = md5::compute(repo.to_string_lossy().as_bytes());
    let dir = crate::portable::data_dir()?.join("snapshots");
    Some(dir.join(format!("{digest:x}.json")))
}

fn read(repo: &Path) -> Option<Snapshot> {
    let path = file(repo)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let snapshot: Snapshot = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (snapshot.version == VERSION && !snapshot.history.is_empty()).then_some(snapshot)
}

fn write(repo: &Path, snapshot: &Snapshot) -> Option<()> {
    let path = file(repo)?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    let bytes = serde_json::to_vec(snapshot).ok()?;
    // Whole or not at all: a reader never sees half of one.
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, bytes).ok()?;
    std::fs::rename(partial, path).ok()
}

impl RepoTab {
    /// Before the first read: the graph and branches as last read, if they were kept. Whether
    /// one was shown is `snapshot_shown` (the opening screen then isn't).
    pub(super) fn show_snapshot(&mut self, ctx: &egui::Context) {
        if self.history_gen > 0 || !self.history.is_empty() || crate::remote::is_remote(&self.path)
        {
            return;
        }
        let Some(snapshot) = read(&self.path) else {
            return;
        };
        self.refs = snapshot.refs;
        self.history = snapshot.history.into_iter().map(Arc::new).collect();
        self.snapshot_shown = true;
        self.rebuild(ctx);
    }

    /// After a read: keeps what it read for the next opening, in the background, and not again
    /// for a while.
    pub(super) fn keep_snapshot(&mut self) {
        if crate::remote::is_remote(&self.path)
            || self.history.is_empty()
            || self.snapshot_kept.is_some_and(|at| at.elapsed() < EVERY)
        {
            return;
        }
        self.snapshot_kept = Some(Instant::now());
        let snapshot = Snapshot {
            version: VERSION,
            refs: self.refs.clone(),
            history: self
                .history
                .iter()
                .take(COMMITS)
                .map(|c| (**c).clone())
                .collect(),
        };
        let repo = self.path.clone();
        let _ = std::thread::Builder::new()
            .name("gitcraft-snapshot".into())
            .spawn(move || write(&repo, &snapshot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_reads_back_as_it_was_written() {
        let snapshot = Snapshot {
            version: VERSION,
            refs: Refs {
                head: "abc".into(),
                current: Some("main".into()),
                ..Default::default()
            },
            history: vec![Commit {
                hash: "abc".into(),
                summary: "First".into(),
                ..Default::default()
            }],
        };
        let bytes = serde_json::to_vec(&snapshot).unwrap_or_default();
        let back: Option<Snapshot> = serde_json::from_slice(&bytes).ok();
        assert!(back.is_some_and(|b| b.refs == snapshot.refs && b.history[0].hash == "abc"));
    }
}
