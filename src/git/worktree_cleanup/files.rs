//! The worktree's files as the cleanup sees them, never following a link: what an untracked
//! file is, how much a folder holds, and checked relative paths.

use std::path::{Component, Path};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::{Footprint, Kind};

/// Untracked files larger than this are not archived.
pub(crate) const MAX_TEXT: u64 = 1 << 20;
/// How long a folder's size is counted, and how many entries, before it says "at least".
const WALK_TIME: Duration = Duration::from_secs(10);
const WALK_ENTRIES: u64 = 2_000_000;

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A relative path as the archive stores it: names joined by `/`, none empty, `.`, `..`, with
/// a separator, drive or NUL in it, and not in `.git`. Anything else is refused, and so is a
/// name Windows would take for another: one ending in a dot or a space (`.git.`), or the
/// short name of `.git` (`GIT~1`).
pub(crate) fn checked_rel(rel: &str) -> Option<Vec<&str>> {
    let names: Vec<&str> = rel.split('/').collect();
    let bad = |n: &&str| {
        n.is_empty()
            || matches!(*n, "." | "..")
            || n.contains(['\\', ':', '\0'])
            || n.eq_ignore_ascii_case(".git")
            || n.ends_with(['.', ' '])
            || short_git(n)
    };
    if rel.is_empty() || names.iter().any(bad) {
        return None;
    }
    // What the platform would make of it: plain names only.
    let normal = Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    normal.then_some(names)
}

/// `git~1` in any case: the 8.3 short name Windows may give `.git`.
fn short_git(name: &str) -> bool {
    name.get(..4).is_some_and(|head| head.eq_ignore_ascii_case("git~"))
        && name.len() > 4
        && name[4..].bytes().all(|b| b.is_ascii_digit())
}

/// What the untracked file `rel` of `root` is, never through a link.
pub(crate) fn classify(root: &Path, rel: &str) -> (u64, Kind) {
    let Some(names) = checked_rel(rel) else {
        return (0, Kind::Other);
    };
    let mut at = root.to_path_buf();
    for (i, name) in names.iter().enumerate() {
        at.push(name);
        let Ok(meta) = std::fs::symlink_metadata(&at) else {
            return (0, Kind::Other);
        };
        if meta.file_type().is_symlink() {
            return (0, Kind::Link);
        }
        let last = i + 1 == names.len();
        if (last && !meta.is_file()) || (!last && !meta.is_dir()) {
            return (0, Kind::Other);
        }
        if last && meta.len() > MAX_TEXT {
            return (meta.len(), Kind::TooLarge);
        }
    }
    match crate::safe_read::read_inside(root, rel, MAX_TEXT) {
        Some(bytes) if bytes.contains(&0) => (bytes.len() as u64, Kind::Binary),
        Some(bytes) => (bytes.len() as u64, Kind::Text(sha256_hex(&bytes))),
        // Swapped for a link or grown past the limit meanwhile.
        None => (0, Kind::Other),
    }
}

/// How much `path` holds (a file or a folder), never following a link, within the limits.
pub(crate) fn footprint(path: &Path) -> Footprint {
    walk(path, Instant::now() + WALK_TIME, WALK_ENTRIES).0
}

/// What `path` holds within the time and entries given, and when anything in it last changed
/// (nanoseconds since 1970).
pub(crate) fn walk(path: &Path, deadline: Instant, limit: u64) -> (Footprint, u128) {
    let mut newest = 0u128;
    let mut size = Footprint {
        complete: true,
        ..Footprint::default()
    };
    let mut stack = vec![path.to_path_buf()];
    let mut seen = 0u64;
    while let Some(at) = stack.pop() {
        seen += 1;
        if seen > limit || Instant::now() > deadline {
            size.complete = false;
            break;
        }
        let Ok(meta) = std::fs::symlink_metadata(&at) else {
            size.complete = false;
            continue;
        };
        let modified = meta.modified().ok();
        let since = modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok());
        newest = newest.max(since.map_or(0, |d| d.as_nanos()));
        if meta.is_dir() {
            match std::fs::read_dir(&at) {
                Ok(entries) => stack.extend(entries.flatten().map(|e| e.path())),
                Err(_) => size.complete = false,
            }
        } else {
            size.files += 1;
            size.bytes += meta.len();
        }
    }
    (size, newest)
}

/// A footprint shown as "1.2 MB", or "≥ 1.2 MB" when the count stopped early.
pub(crate) fn describe(size: Footprint) -> String {
    let bytes = human(size.bytes);
    if size.complete {
        bytes
    } else {
        format!("≥ {bytes}")
    }
}

/// `bytes` as "512 B", "3.4 KB", "1.2 MB", "5.0 GB".
pub(crate) fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
