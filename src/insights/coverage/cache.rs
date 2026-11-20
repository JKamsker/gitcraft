//! Parsed reports kept for reuse: opening file after file of a diff with the coverage gutter
//! on, or refreshing it, would otherwise read and parse the whole report (up to
//! [`MAX_REPORT`]) every time. A report is reused only while its file is the same one: same
//! path, size and modification time. A report on another machine (no local metadata) isn't
//! kept. Background threads only: nothing here runs on the UI thread.

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::*;

/// How many parsed reports are kept (the most recently used first); more in tests, where many
/// repositories read reports at once and one test's must not push out another's.
const KEPT: usize = if cfg!(test) { 64 } else { 2 };

/// Which file a parsed report was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
}

static KEPT_REPORTS: Mutex<Vec<(Key, Arc<Report>)>> = Mutex::new(Vec::new());

#[cfg(test)]
thread_local! {
    /// Reports parsed on this thread: what the tests count to see one reused.
    pub(super) static PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The key of the report at `path` here, when its metadata can be read.
fn key_of(repo: &Path, path: &Path) -> Option<Key> {
    if crate::remote::is_remote(repo) {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    Some(Key {
        path: path.to_path_buf(),
        len: meta.len(),
        modified: meta.modified().ok()?,
    })
}

/// The report `name` at `path` parsed, from what was kept when its file hasn't changed.
pub(super) fn parsed(repo: &Path, path: &Path, name: &str) -> Result<Arc<Report>, String> {
    let key = key_of(repo, path);
    if let Some(key) = &key
        && let Ok(mut kept) = KEPT_REPORTS.lock()
        && let Some(at) = kept.iter().position(|(k, _)| k == key)
    {
        let entry = kept.remove(at);
        let report = entry.1.clone();
        kept.insert(0, entry);
        return Ok(report);
    }
    let report = Arc::new(read_and_parse(repo, path, name)?);
    if let Some(key) = key
        && let Ok(mut kept) = KEPT_REPORTS.lock()
    {
        kept.retain(|(k, _)| k.path != key.path);
        kept.insert(0, (key, report.clone()));
        kept.truncate(KEPT);
    }
    Ok(report)
}

/// Reads the report at `path` and parses it, refusing one bigger than [`MAX_REPORT`].
fn read_and_parse(repo: &Path, path: &Path, name: &str) -> Result<Report, String> {
    let too_big = |len: u64| {
        format!(
            "{name} is {} MB, more than the {} MB read",
            len >> 20,
            MAX_REPORT >> 20
        )
    };
    if !crate::remote::is_remote(repo)
        && let Ok(meta) = std::fs::metadata(path)
        && meta.len() > MAX_REPORT
    {
        return Err(too_big(meta.len()));
    }
    let bytes = crate::remote::read(path).map_err(|e| format!("{name}: {e}"))?;
    if bytes.len() as u64 > MAX_REPORT {
        return Err(too_big(bytes.len() as u64));
    }
    #[cfg(test)]
    PARSES.with(|p| p.set(p.get() + 1));
    parse(name, &String::from_utf8_lossy(&bytes)).map_err(|e| format!("{name}: {e}"))
}

static TOPLEVELS: Mutex<Vec<(PathBuf, String)>> = Mutex::new(Vec::new());

/// The top folder of the repository at `repo` (as git names it), asked of git once.
pub(super) fn toplevel(repo: &Path) -> Result<String, String> {
    if let Ok(known) = TOPLEVELS.lock()
        && let Some((_, root)) = known.iter().find(|(r, _)| r == repo)
    {
        return Ok(root.clone());
    }
    let root = git::run(Some(repo), &["rev-parse", "--show-toplevel"])?
        .trim()
        .to_owned();
    if let Ok(mut known) = TOPLEVELS.lock() {
        known.insert(0, (repo.to_path_buf(), root.clone()));
        known.truncate(16);
    }
    Ok(root)
}
