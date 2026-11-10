//! Patch export.

use super::*;

/// [`save_patch`] where git runs for `repo`, and the file's path as git there is given it: for
/// a repository in WSL or over SSH, a file on that machine.
pub fn save_patch_for(repo: &Path, patch: &str) -> Result<String, String> {
    match crate::remote::parse(repo) {
        Some(target) => crate::remote::host_temp_file(&target, "patch.patch", patch.as_bytes()),
        None => save_patch(patch).map(|p| p.to_string_lossy().into_owned()),
    }
}

/// Saves `patch` to a temporary file for `git apply`, which reads it from there (so the
/// operation is a plain list of git commands, which undo can replay). Patches older than a day
/// are cleared on the way.
pub fn save_patch(patch: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    // One folder per user: it is closed to everyone else below, so a shared one would leave
    // every user but its first unable to stage a hunk.
    let user: String = ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|u| !u.is_empty()))
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let dir = std::env::temp_dir().join(if user.is_empty() {
        "gitcraft-patches".to_owned()
    } else {
        format!("gitcraft-patches-{user}")
    });
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // `/tmp` is everyone's: a folder someone else made there (or a link to one) could have the
    // patch read, or swapped before `git apply` reads it. Only its owner can close it to others.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let not_ours = |e: std::io::Error| {
            crate::i18n::message(
                "backend-isn-t-gitcraft-s-e-77dde3",
                &[
                    ("path", (dir.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        };
        if std::fs::symlink_metadata(&dir)
            .map_err(not_ours)?
            .file_type()
            .is_symlink()
        {
            return Err(crate::i18n::message(
                "backend-is-a-link-6a3d7b",
                &[("path", (dir.display()).to_string().into())],
            ));
        }
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(not_ours)?;
    }
    // This run's own patches stay, however old: undoing a discard of lines applies its patch
    // again, the only copy of those lines.
    let ours = format!("{}-", std::process::id());
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 24 * 3600);
            if old && !e.file_name().to_string_lossy().starts_with(&ours) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{ours}{}-{n}.patch", now()));
    // A new file, never one already there (or a link someone left in its place).
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?
            .write_all(patch.as_bytes())
    };
    write().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
