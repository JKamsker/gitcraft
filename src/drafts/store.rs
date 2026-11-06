//! The drafts' files: one per repository in the `drafts` folder of gitcraft's data folder,
//! named after the repository and a hash of its path. Each is written whole to a temporary file
//! beside it and renamed over the old one, so a crash mid-write leaves the previous version.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use super::limits::MAX_FILE;
use super::{DraftFile, FORMAT, RepoDrafts};

/// What temporary files end in; one left behind by a crash is never read, and is cleared away.
const TEMP: &str = ".tmp";
/// Drafts not changed for this long (about three months) are let go of.
const KEEP_FOR: Duration = Duration::from_secs(60 * 60 * 24 * 90);

/// The folder of the drafts under the data folder `root`.
pub(super) fn folder(root: &Path) -> PathBuf {
    root.join("drafts")
}

/// The file of `repo`'s drafts in `folder`. The name shows which repository it is, and the
/// hash tells apart repositories of the same name (and the same folder on two machines).
pub(super) fn file_of(folder: &Path, repo: &Path) -> PathBuf {
    let key = repo.to_string_lossy();
    let hash = format!("{:x}", md5::compute(key.as_bytes()));
    let name: String = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    folder.join(format!("{name}-{}.json", &hash[..16]))
}

/// The drafts in `file` for `repo`: empty when there is no file, or it is another
/// repository's (two paths with the same hash) or another version's.
pub(super) fn read(file: &Path, repo: &Path) -> Result<RepoDrafts, String> {
    let mut handle = match fs::File::open(file) {
        Ok(handle) => handle,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RepoDrafts::default()),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let mut bytes = Vec::new();
    // One byte more than the limit tells a file too large apart from one just at it.
    Read::by_ref(&mut handle)
        .take(MAX_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    if bytes.len() > MAX_FILE {
        return Err(format!(
            "{} is larger than {} MB, so it wasn't read",
            file.display(),
            MAX_FILE / (1024 * 1024)
        ));
    }
    let parsed: DraftFile = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is damaged ({e}), so it wasn't read", file.display()))?;
    if parsed.format != FORMAT || parsed.repo != repo {
        return Ok(RepoDrafts::default());
    }
    Ok(parsed.drafts)
}

/// Writes `bytes` to `file` whole or not at all.
pub(super) fn write_atomic(file: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = write_temp(file, bytes)?;
    fs::rename(&temp, file).map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("{}: {e}", file.display())
    })
}

/// The first half of [`write_atomic`]: `bytes` in a new temporary file beside `file`, flushed
/// to the disk. Until it is renamed, `file` is as it was.
pub(super) fn write_temp(file: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let folder = file.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let mut name = file.file_name().unwrap_or_default().to_owned();
    name.push(format!(".{}-{n}{TEMP}", std::process::id()));
    let temp = folder.join(name);
    let written = fs::File::create(&temp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()
    });
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        format!("{}: {e}", temp.display())
    })?;
    Ok(temp)
}

/// Removes `file`; there being none is no error.
pub(super) fn remove(file: &Path) -> Result<(), String> {
    match fs::remove_file(file) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", file.display()))
        }
        _ => Ok(()),
    }
}

/// Clears from `folder` the temporary files a crash left, and the drafts of repositories not
/// changed for [`KEEP_FOR`] (reading a file doesn't touch it, but it is
/// written again as soon as anything is typed).
pub(super) fn tidy(folder: &Path) {
    let Ok(list) = fs::read_dir(folder) else {
        return;
    };
    let now = SystemTime::now();
    for entry in list.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|at| now.duration_since(at).ok())
            .unwrap_or_default();
        // A temporary file of another running gitcraft may be about to be renamed.
        let stale_temp = name.ends_with(TEMP) && age > Duration::from_secs(60 * 60);
        let old_drafts = name.ends_with(".json") && age > KEEP_FOR;
        if stale_temp || old_drafts {
            let _ = fs::remove_file(path);
        }
    }
}
