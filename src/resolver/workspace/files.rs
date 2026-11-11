use super::*;

pub(super) fn plain(top: &Path, path: &str) -> Result<bool, String> {
    let mut at = top.to_path_buf();
    // The parts the system goes through to open it: on Unix a `\` is part of a name.
    for part in Path::new(path).components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("{path} goes through a link"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("couldn't look at {path}: {e}")),
        }
    }
    Ok(true)
}

/// Reads `path` of the checkout at `top`: `None` when there is no such file. One reached
/// through a link, or that isn't a file, is an error.
pub(super) fn read(top: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    if !plain(top, path)? {
        return Ok(None);
    }
    // Read so that what was looked at is what is read: a link swapped in on the way since
    // (by something the agent left running) isn't followed.
    crate::safe_read::read_inside(top, path, u64::MAX - 1)
        .map(Some)
        .ok_or_else(|| format!("{path} isn't a plain file, or couldn't be read"))
}

/// Writes `bytes` to `path` of the checkout at `top` (removes it for `None`), never through a
/// link.
pub(super) fn write(top: &Path, path: &str, bytes: Option<&[u8]>) -> Result<(), String> {
    plain(top, path)?;
    let file = top.join(path);
    match bytes {
        Some(bytes) => {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("couldn't make the folder of {path}: {e}"))?;
            }
            replace(&file, bytes).map_err(|e| format!("couldn't write {path}: {e}"))
        }
        None => match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {path}: {e}")),
        },
    }
}

/// Writes `bytes` to `file` whole or not at all: into a file beside it first, then put in its
/// place, with its permissions (an executable stays one). One that fails leaves it as it was.
pub(super) fn replace(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = file.with_file_name(format!(".gitcraft-{}-{n}", std::process::id()));
    replace_with_temp(file, &temp, bytes)
}

pub(super) fn replace_with_temp(file: &Path, temp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    // Only an opened file belongs to us. A create_new collision must remain untouched.
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    let written = (|| {
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        // Only Unix has more to them than read-only (which would keep it from being replaced).
        #[cfg(unix)]
        if let Ok(meta) = std::fs::symlink_metadata(file) {
            std::fs::set_permissions(temp, meta.permissions())?;
        }
        std::fs::rename(temp, file)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    written
}

/// A path git status gave, checked to stay inside the checkout.
pub(super) fn checked(path: &str) -> Result<&str, String> {
    crate::mcp::args::repo_path(path)
}

// git in the agent's scratch worktree, without the repository's hooks nor its recorded
// resolutions: that copy is gitcraft's own, and nothing the user set up to run on a checkout
// or a merge is for it.
