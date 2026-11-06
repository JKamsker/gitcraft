//! Keeping a folder's timeline on disk, in gitcraft's own data folder (not in the repository:
//! nothing to commit by mistake, and the same for a repository elsewhere, in WSL or over SSH).
//! One JSON file per folder, written whole to a file beside it and moved over it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{Entry, RefState, Timeline};

/// The format of the file; a newer one than this is not read (nor written over).
const VERSION: u32 = 1;

/// A timeline as kept on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    /// The folder it is for, as the tab names it (for whoever reads the file).
    #[serde(default)]
    pub(crate) folder: String,
    pub(crate) entries: Vec<Entry>,
    pub(crate) current: Option<u64>,
    pub(crate) seen: Option<RefState>,
    pub(crate) next_id: u64,
}

impl Saved {
    pub(crate) fn of(timeline: &Timeline, folder: &Path) -> Self {
        Self {
            version: VERSION,
            folder: folder.display().to_string(),
            entries: timeline.entries.clone(),
            current: timeline.current,
            seen: timeline.seen.clone(),
            next_id: timeline.next_id,
        }
    }
}

/// Where the timelines are kept.
fn root() -> Option<PathBuf> {
    #[cfg(test)]
    {
        Some(std::env::temp_dir().join(format!("gitcraft-timelines-{}", std::process::id())))
    }
    #[cfg(not(test))]
    {
        eframe::storage_dir("gitcraft").map(|d| d.join("timeline"))
    }
}

/// A short name for `folder` that is the same every time: its file, and the part of the
/// names of its kept refs that tells it from another worktree's (see `snapshot`).
pub(crate) fn key(folder: &Path) -> String {
    let digest = md5::compute(folder.to_string_lossy().as_bytes());
    format!("{digest:x}")[..16].to_owned()
}

pub(crate) fn file(folder: &Path) -> Option<PathBuf> {
    root().map(|r| r.join(format!("{}.json", key(folder))))
}

/// What was kept for `folder`: nothing when there is no file yet. A file that can't be read
/// (written by a newer gitcraft, or damaged) is an error, and is not written over.
pub(crate) fn load(folder: &Path) -> Result<Saved, String> {
    let Some(path) = file(folder) else {
        return Ok(Saved::default());
    };
    load_from(&path)
}

pub(crate) fn load_from(path: &Path) -> Result<Saved, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Saved::default()),
        Err(e) => {
            return Err(format!(
                "The timeline in {} can't be read: {e}",
                path.display()
            ));
        }
    };
    let saved: Saved = serde_json::from_str(&text).map_err(|e| {
        format!(
            "The timeline in {} is damaged ({e}); it is left as it is and a new one isn't kept",
            path.display()
        )
    })?;
    if saved.version > VERSION {
        return Err(format!(
            "The timeline in {} was written by a newer gitcraft; it is left as it is and this \
             one doesn't keep one",
            path.display()
        ));
    }
    Ok(saved)
}

/// Numbers the writes asked for, of every tab: a tab opened again on the same folder writes
/// after the one closed.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The number of a write asked for now.
pub(crate) fn next_generation() -> u64 {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}

/// The newest write done of each file: an older one that runs late is skipped.
static WRITTEN: Mutex<Option<HashMap<PathBuf, u64>>> = Mutex::new(None);

/// Writes `saved` for `folder`, unless a write numbered after `generation` already went
/// out. Meant for a background thread.
pub(crate) fn save(folder: &Path, saved: &Saved, generation: u64) -> Result<(), String> {
    let Some(path) = file(folder) else {
        return Ok(());
    };
    save_to(&path, saved, generation)
}

pub(crate) fn save_to(path: &Path, saved: &Saved, generation: u64) -> Result<(), String> {
    // One write at a time: two threads moving their files over the same one could leave the
    // older last.
    let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let map = written.get_or_insert_with(HashMap::new);
    if map.get(path).is_some_and(|g| *g > generation) {
        return Ok(());
    }
    let fail = |e: std::io::Error| format!("The timeline can't be kept in {}: {e}", path.display());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    let text =
        serde_json::to_string(saved).map_err(|e| format!("The timeline can't be kept: {e}"))?;
    let part = path.with_extension("json.part");
    std::fs::write(&part, text).map_err(fail)?;
    std::fs::rename(&part, path).map_err(fail)?;
    map.insert(path.to_owned(), generation);
    Ok(())
}
