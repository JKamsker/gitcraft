//! Checking a worktree against the fence: what it changed since it left the main branch
//! (committed or not), and the files it made that git doesn't track or ignores. The result is
//! what is checked, not the rules written for the agents, which can leak.

use std::path::Path;

use super::pattern::{Pattern, fenced};

/// How a fenced path was touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum How {
    /// Changed by a commit of the worktree's branch since it left the base.
    Committed,
    /// Changed in the working tree or the index, not committed.
    Uncommitted,
    /// A new file git doesn't track.
    Untracked,
    /// A file git ignores, which `git worktree add` never makes: made in the worktree since.
    Ignored,
}

impl How {
    pub fn label(self) -> &'static str {
        match self {
            How::Committed => "committed",
            How::Uncommitted => "not committed",
            How::Untracked => "untracked",
            How::Ignored => "ignored by git",
        }
    }
}

/// A fenced path a worktree touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub path: String,
    pub how: How,
    /// The fence entry it is inside of.
    pub pattern: String,
}

/// The fenced paths the worktree at `worktree` touched since its branch left `base` (the
/// main worktree's commit).
pub fn check(worktree: &Path, base: &str, patterns: &[Pattern]) -> Result<Vec<Violation>, String> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let failed = |e: String| {
        format!(
            "Could not check {} against the fence: {e}",
            crate::remote::display(worktree)
        )
    };
    let mut found: Vec<Violation> = Vec::new();
    let mut add = |path: &str, how: How| {
        if let Some(p) = fenced(patterns, path.trim_end_matches('/'))
            && !found.iter().any(|v| v.path == path && v.how == how)
        {
            found.push(Violation {
                path: path.to_owned(),
                how,
                pattern: p.text.clone(),
            });
        }
    };
    for path in committed(worktree, base).map_err(failed)? {
        add(&path, How::Committed);
    }
    for (path, how) in working(worktree).map_err(failed)? {
        add(&path, how);
    }
    found.sort_by(|a, b| (a.how, &a.path).cmp(&(b.how, &b.path)));
    Ok(found)
}

/// The paths the commits of the worktree's HEAD changed since it left `base` (both names of
/// a rename).
fn committed(worktree: &Path, base: &str) -> Result<Vec<String>, String> {
    if base.is_empty() || base.starts_with('-') {
        return Ok(Vec::new());
    }
    let fork = crate::git::run(
        Some(worktree),
        &["merge-base", "--end-of-options", base, "HEAD"],
    )?;
    let fork = fork.trim();
    if fork.is_empty() {
        return Ok(Vec::new());
    }
    let out = crate::git::run(
        Some(worktree),
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--end-of-options",
            fork,
            "HEAD",
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// What `git status` shows in the worktree: changes, untracked and ignored files.
fn working(worktree: &Path) -> Result<Vec<(String, How)>, String> {
    let out = crate::git::run(
        Some(worktree),
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
            "--no-renames",
        ],
    )?;
    Ok(parse_status(&out))
}

/// `git status --porcelain=v1 -z --no-renames`: `XY path`, each ended by a NUL.
pub fn parse_status(out: &str) -> Vec<(String, How)> {
    out.split('\0')
        .filter(|e| e.len() > 3)
        .map(|entry| {
            let (code, path) = entry.split_at(3);
            let how = match code.get(..2) {
                Some("??") => How::Untracked,
                Some("!!") => How::Ignored,
                _ => How::Uncommitted,
            };
            (path.to_owned(), how)
        })
        .collect()
}
