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
    // Checked before anything is unset: a refused entry leaves the list as it was.
    if let Some(bad) = entries.iter().find(|e| e.contains('\n')) {
        return Err(failed(format!("{bad:?} has a line break")));
    }
    if let Some(why) = entries
        .iter()
        .find_map(|e| super::pattern::Pattern::check(e))
    {
        return Err(failed(why));
    }
    // 5: there was nothing to unset.
    crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--unset-all", KEY],
        &[0, 5],
    )
    .map_err(failed)?;
    for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        crate::git::run(Some(repo), &["config", "--local", "--add", KEY, entry]).map_err(failed)?;
    }
    Ok(())
}

const TARGETS: &str = "gitcraft.fenceWrite";

/// Which configurations besides Claude Code's the fence is written into (none unless asked:
/// `.cursorignore` and `AGENTS.md` are files a repository usually tracks).
pub fn read_targets(repo: &Path) -> super::configs::Targets {
    let values = crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--get-all", TARGETS],
        &[0, 1],
    )
    .unwrap_or_default();
    super::configs::Targets {
        cursor: values.lines().any(|l| l.trim() == "cursor"),
        agents_md: values.lines().any(|l| l.trim() == "agents"),
    }
}

pub fn write_targets(repo: &Path, targets: super::configs::Targets) -> Result<(), String> {
    let failed = |e: String| format!("Could not save where the fence is written: {e}");
    crate::git::run_ok_codes(
        Some(repo),
        &["config", "--local", "--unset-all", TARGETS],
        &[0, 5],
    )
    .map_err(failed)?;
    for (on, value) in [(targets.cursor, "cursor"), (targets.agents_md, "agents")] {
        if on {
            crate::git::run(Some(repo), &["config", "--local", "--add", TARGETS, value])
                .map_err(failed)?;
        }
    }
    Ok(())
}
