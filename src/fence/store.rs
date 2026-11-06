//! The fence's list, one per repository: in its own git config (`gitcraft.fence`, one value
//! per entry), which its worktrees share, read and written through the git command layer.

use std::path::Path;

const KEY: &str = "gitcraft.fence";

/// What a repository with no list yet is offered to start from.
pub const SUGGESTED: [&str; 10] = [
    ".env",
    ".env.*",
    "secrets/",
    "*.pem",
    "*.key",
    ".github/workflows/",
    "migrations/",
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
];

/// The list (empty when none is set).
pub fn read(repo: &Path) -> Result<Vec<String>, String> {
    // 1: the key isn't set.
    let out = crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--get-all", KEY],
        &[0, 1],
    )
    .map_err(|e| format!("Could not read the agent fence: {e}"))?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Replaces the list with `entries` (empty ones dropped).
pub fn write(repo: &Path, entries: &[String]) -> Result<(), String> {
    let failed = |e: String| format!("Could not save the agent fence: {e}");
    // 5: there was nothing to unset.
    crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--unset-all", KEY],
        &[0, 5],
    )
    .map_err(failed)?;
    for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        if entry.contains('\n') {
            return Err(failed(format!("{entry:?} has a line break")));
        }
        crate::git::run(Some(repo), &["config", "--local", "--add", KEY, entry]).map_err(failed)?;
    }
    Ok(())
}
