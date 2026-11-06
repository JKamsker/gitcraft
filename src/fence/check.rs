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
/// main worktree's commit; with none, as in a bare repository, the remote's default branch or
/// `main` / `master`). `own`: files gitcraft writes there itself, left out.
pub fn check(
    worktree: &Path,
    base: &str,
    patterns: &[Pattern],
    own: &[&str],
) -> Result<Vec<Violation>, String> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let failed = |e: String| {
        format!(
            "Could not check {} against the agent fence: {e}",
            crate::remote::display(worktree)
        )
    };
    let base = if base.is_empty() {
        fallback_base(worktree)
            .ok_or_else(|| failed("there is no main branch to compare with".to_owned()))?
    } else {
        base.to_owned()
    };
    let mut found = Found::new(patterns, own);
    for path in committed(worktree, &base, "HEAD").map_err(failed)? {
        found.add(&path, How::Committed);
    }
    for (path, how) in working(worktree).map_err(failed)? {
        found.add(&path, how);
    }
    for path in ignored(worktree, patterns).map_err(failed)? {
        found.add(&path, How::Ignored);
    }
    Ok(found.done())
}

/// The fenced paths the commits of `tip` bring into `into` (what merging or rebasing onto
/// it would): those changed since the two diverged.
pub fn check_ref(
    repo: &Path,
    tip: &str,
    into: &str,
    patterns: &[Pattern],
) -> Result<Vec<Violation>, String> {
    let mut found = Found::new(patterns, &[]);
    for path in committed(repo, into, tip)
        .map_err(|e| format!("Could not check {tip} against the agent fence: {e}"))?
    {
        found.add(&path, How::Committed);
    }
    Ok(found.done())
}

/// What a check found so far.
struct Found<'a> {
    patterns: &'a [Pattern],
    own: &'a [&'a str],
    list: Vec<Violation>,
}

impl<'a> Found<'a> {
    fn new(patterns: &'a [Pattern], own: &'a [&'a str]) -> Self {
        Self {
            patterns,
            own,
            list: Vec::new(),
        }
    }

    fn add(&mut self, path: &str, how: How) {
        let bare = path.trim_end_matches('/');
        if self.own.contains(&bare) {
            return;
        }
        if let Some(p) = fenced(self.patterns, bare)
            && !self.list.iter().any(|v| v.path == path && v.how == how)
        {
            self.list.push(Violation {
                path: path.to_owned(),
                how,
                pattern: p.text.clone(),
            });
        }
    }

    fn done(mut self) -> Vec<Violation> {
        self.list
            .sort_by(|a, b| (a.how, &a.path).cmp(&(b.how, &b.path)));
        self.list
    }
}

/// The branch a worktree is compared with when the main worktree has no commit (bare).
fn fallback_base(worktree: &Path) -> Option<String> {
    [
        "refs/remotes/origin/HEAD",
        "refs/heads/main",
        "refs/heads/master",
    ]
    .iter()
    .find_map(|r| {
        crate::git::run(Some(worktree), &["rev-parse", "--verify", "--quiet", r])
            .ok()
            .map(|h| h.trim().to_owned())
            .filter(|h| !h.is_empty())
    })
}

/// The paths the commits of `tip` changed since it left `base` (both names of a rename).
fn committed(dir: &Path, base: &str, tip: &str) -> Result<Vec<String>, String> {
    if base.starts_with('-') || tip.starts_with('-') {
        return Err(format!("{base} or {tip} is not a revision"));
    }
    let fork = crate::git::run(Some(dir), &["merge-base", "--end-of-options", base, tip])?;
    let fork = fork.trim();
    if fork.is_empty() {
        return Err("it shares no history with the main branch".to_owned());
    }
    let out = crate::git::run(
        Some(dir),
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
            tip,
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// What `git status` shows in the worktree: changes and untracked files.
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
            "--no-renames",
        ],
    )?;
    Ok(parse_status(&out))
}

/// The ignored files the fence covers, each of them, also inside ignored folders (which
/// `git status` shows as one entry): listed by the fence's own pathspecs, so that git looks
/// only for those.
fn ignored(worktree: &Path, patterns: &[Pattern]) -> Result<Vec<String>, String> {
    let specs: Vec<String> = patterns.iter().flat_map(Pattern::pathspecs).collect();
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "-z",
        "--",
    ];
    args.extend(specs.iter().map(String::as_str));
    let out = crate::git::run(Some(worktree), &args)?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
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
