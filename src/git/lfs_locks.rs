//! Git LFS file locks: who holds which file, taking and giving back a lock, and the files the
//! repository marks `lockable`. Every call goes to the LFS server through `git lfs`, so it runs
//! on the machine the repository is on (WSL, SSH) like any other git call.

use std::path::Path;

use serde::Deserialize;

use super::{Credentials, run_unattended, run_with};

/// A file locked on the LFS server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfsLock {
    /// The server's id for the lock, which unlocking names.
    pub id: String,
    /// The file, from the repository's root, with `/` between folders.
    pub path: String,
    /// Who holds it, as the server names them.
    pub owner: String,
    /// When it was taken (Unix seconds), when the server says.
    pub locked_at: Option<i64>,
    /// Held by whoever this repository signs in as: `None` when the server couldn't say (it
    /// lets the locks be listed but not verified).
    pub ours: Option<bool>,
}

impl LfsLock {
    /// Held by someone else, for sure.
    pub fn theirs(&self) -> bool {
        self.ours == Some(false)
    }
}

#[derive(Deserialize)]
struct RawOwner {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct RawLock {
    id: String,
    path: String,
    #[serde(default)]
    owner: Option<RawOwner>,
    #[serde(default)]
    locked_at: Option<String>,
}

#[derive(Deserialize)]
struct RawVerified {
    #[serde(default)]
    ours: Vec<RawLock>,
    #[serde(default)]
    theirs: Vec<RawLock>,
}

impl RawLock {
    fn into_lock(self, ours: Option<bool>) -> LfsLock {
        LfsLock {
            id: self.id,
            // git-lfs on Windows may say `a\\b`; elsewhere a backslash is part of a name.
            path: if cfg!(windows) {
                self.path.replace('\\', "/")
            } else {
                self.path
            },
            owner: self.owner.map(|o| o.name).unwrap_or_default(),
            locked_at: self.locked_at.as_deref().and_then(parse_rfc3339),
            ours,
        }
    }
}

/// How the locks are asked for: by the user (who may be asked to sign in), or in the
/// background, where nobody is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    ByUser,
    Background,
}

fn lfs(
    repo: &Path,
    args: &[&str],
    creds: Option<&Credentials>,
    asked: Asked,
) -> Result<String, String> {
    let result = match asked {
        Asked::ByUser => run_with(Some(repo), args, creds),
        Asked::Background => run_unattended(Some(repo), args, creds),
    };
    result.map_err(|e| lfs_error(&e))
}

/// git-lfs's error as one readable line: its last line that says something, which names the
/// server's reason ("Lock exists", "Cannot unlock file with uncommitted changes").
fn lfs_error(e: &str) -> String {
    let line = e
        .lines()
        .map(str::trim)
        .rev()
        // With `--json` git-lfs still prints JSON (`[]`) on stdout when it fails.
        .find(|l| !l.is_empty() && !l.starts_with("Errors logged to") && !l.starts_with(['[', '{']))
        .unwrap_or("git-lfs failed without saying why");
    line.strip_prefix("Error: ").unwrap_or(line).to_owned()
}

/// Every lock on the server, each marked ours or theirs when the server can verify them
/// (`git lfs locks --verify`), else just listed.
pub fn lfs_locks(
    repo: &Path,
    creds: Option<&Credentials>,
    asked: Asked,
) -> Result<Vec<LfsLock>, String> {
    let verified = lfs(repo, &["lfs", "locks", "--verify", "--json"], creds, asked);
    match verified {
        // A server that can't verify answers with nothing, as git-lfs says (on stderr only),
        // as one with no locks does: the plain list tells them apart.
        Ok(json) => {
            let locks = parse_verified(&json)?;
            if !locks.is_empty() {
                return Ok(locks);
            }
            let json = lfs(repo, &["lfs", "locks", "--json"], creds, asked)?;
            parse_listed(&json)
        }
        // Signing in is what's missing: listing would be refused for the same reason.
        Err(e) if super::is_auth_error(&e) => Err(e),
        Err(_) => {
            let json = lfs(repo, &["lfs", "locks", "--json"], creds, asked)?;
            parse_listed(&json)
        }
    }
}

/// `git lfs locks --verify --json`: `{"ours": [...], "theirs": [...]}`.
pub(crate) fn parse_verified(json: &str) -> Result<Vec<LfsLock>, String> {
    let raw: RawVerified = serde_json::from_str(json.trim())
        .map_err(|e| format!("git-lfs listed the locks in a way gitcraft can't read: {e}"))?;
    let mut locks: Vec<LfsLock> = raw
        .ours
        .into_iter()
        .map(|l| l.into_lock(Some(true)))
        .chain(raw.theirs.into_iter().map(|l| l.into_lock(Some(false))))
        .collect();
    locks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(locks)
}

/// `git lfs locks --json`: a list of locks, nobody's known to be ours.
pub(crate) fn parse_listed(json: &str) -> Result<Vec<LfsLock>, String> {
    let json = json.trim();
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let raw: Vec<RawLock> = serde_json::from_str(json)
        .map_err(|e| format!("git-lfs listed the locks in a way gitcraft can't read: {e}"))?;
    let mut locks: Vec<LfsLock> = raw.into_iter().map(|l| l.into_lock(None)).collect();
    locks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(locks)
}

/// Locks `path` (from the repository's root) for whoever this repository signs in as.
pub fn lfs_lock(repo: &Path, path: &str, creds: Option<&Credentials>) -> Result<LfsLock, String> {
    let json = lfs(
        repo,
        &["lfs", "lock", "--json", "--", path],
        creds,
        Asked::ByUser,
    )?;
    // git-lfs answers with the lock, alone or in a list of one.
    let json = json.trim();
    let raw: RawLock = serde_json::from_str::<Vec<RawLock>>(json)
        .ok()
        .and_then(|v| v.into_iter().next())
        .map(Ok)
        .unwrap_or_else(|| serde_json::from_str(json))
        .map_err(|e| format!("git-lfs said {path} was locked, but not how: {e}"))?;
    Ok(raw.into_lock(Some(true)))
}

/// Gives back the locks `ids`, one after another; `force` takes someone else's (which the
/// server allows admins only). Every lock is tried: the error names those that stayed.
pub fn lfs_unlock(
    repo: &Path,
    locks: &[(String, String)],
    force: bool,
    creds: Option<&Credentials>,
) -> Result<(), String> {
    let mut failed = Vec::new();
    for (id, path) in locks {
        let mut args = vec!["lfs", "unlock", "--json", "--id", id.as_str()];
        if force {
            args.push("--force");
        }
        if let Err(e) = lfs(repo, &args, creds, Asked::ByUser) {
            failed.push(format!("{path}: {e}"));
        }
    }
    match failed.len() {
        0 => Ok(()),
        1 => Err(failed.remove(0)),
        n => Err(format!("{n} locks stayed: {}", failed.join("; "))),
    }
}

/// Makes files matching `pattern` lockable (`git lfs track --lockable`, which writes
/// `.gitattributes`): they are kept in LFS, read-only until locked.
pub fn track_lockable(repo: &Path, pattern: &str) -> Result<String, String> {
    if pattern.trim().is_empty() {
        return Err("Type a pattern, such as *.psd".to_owned());
    }
    super::run(
        Some(repo),
        &["lfs", "track", "--lockable", "--", pattern.trim()],
    )
    .map_err(|e| lfs_error(&e))
}

/// The patterns `.gitattributes` at the root marks `lockable`.
pub fn lockable_patterns(repo: &Path) -> Vec<String> {
    crate::remote::read_to_string(&crate::remote::join(repo, ".gitattributes"))
        .map(|text| lockable_in(&text))
        .unwrap_or_default()
}

/// The patterns of a `.gitattributes` text with the `lockable` attribute set.
pub(crate) fn lockable_in(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (pattern, attrs) = split_pattern(l)?;
            attrs
                .split_whitespace()
                .any(|a| a == "lockable" || a == "lockable=true")
                .then(|| pattern.to_owned())
        })
        .collect()
}

/// A `.gitattributes` line's pattern (quoted, `"my file.bin"`, when it has spaces) and the
/// attributes after it.
fn split_pattern(line: &str) -> Option<(&str, &str)> {
    if let Some(rest) = line.strip_prefix('"') {
        let mut escaped = false;
        let end = rest.char_indices().find_map(|(i, c)| {
            let close = c == '"' && !escaped;
            escaped = c == '\\' && !escaped;
            close.then_some(i)
        })?;
        return Some((&line[..end + 2], &rest[end + 1..]));
    }
    Some(line.split_once(char::is_whitespace).unwrap_or((line, "")))
}

/// Which of `paths` git sees as lockable (`git check-attr lockable`), every attributes file
/// counted. Asked in batches, so that a long list of changes stays within a command line.
pub fn lockable_paths(repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for batch in batches(paths, ARGS_BUDGET) {
        let mut args = vec!["check-attr", "-z", "lockable", "--"];
        args.extend(batch.iter().map(String::as_str));
        let out = super::run(Some(repo), &args)?;
        found.extend(parse_check_attr(&out));
    }
    Ok(found)
}

/// How many bytes of paths one command line takes: well within Windows' 32 767 characters.
const ARGS_BUDGET: usize = 16 * 1024;

/// `paths` in runs whose lengths add up to at most `budget` (a longer path on its own).
pub(crate) fn batches(paths: &[String], budget: usize) -> Vec<&[String]> {
    let mut out = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (i, p) in paths.iter().enumerate() {
        let len = p.len() + 3;
        if used + len > budget && i > start {
            out.push(&paths[start..i]);
            (start, used) = (i, 0);
        }
        used += len;
    }
    if start < paths.len() {
        out.push(&paths[start..]);
    }
    out
}

/// The files the last push of the checked-out branch carried: what changed between where its
/// remote branch was before (its reflog) and where it is now. None for a first push, whose
/// remote branch has no before.
pub fn pushed_paths(repo: &Path) -> Result<Vec<String>, String> {
    let Ok(before) = super::run(
        Some(repo),
        &["rev-parse", "--verify", "-q", "@{upstream}@{1}"],
    ) else {
        return Ok(Vec::new());
    };
    let out = super::run(
        Some(repo),
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            before.trim(),
            "@{upstream}",
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// `git check-attr -z`: `path NUL attribute NUL value NUL`, again and again; the paths whose
/// value is set.
pub(crate) fn parse_check_attr(out: &str) -> Vec<String> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .chunks_exact(3)
        .filter(|f| f[2] == "set" || f[2] == "true")
        .map(|f| f[0].to_owned())
        .collect()
}

/// `2026-11-01T10:00:00Z`, `2026-11-01T10:00:00.123+02:00`… as Unix seconds.
pub(crate) fn parse_rfc3339(s: &str) -> Option<i64> {
    let s = s.trim();
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut d = date.splitn(3, '-');
    let (y, m, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let (time, offset) = split_offset(rest)?;
    let mut t = time.splitn(3, ':');
    let (h, min): (i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    let sec: i64 = t
        .next()
        .map(|s| s.split('.').next().unwrap_or("0").parse().ok())
        .unwrap_or(Some(0))?;
    if !(0..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || !(1..=31).contains(&day)
        || h > 23
        || min > 59
        || sec > 60
    {
        return None;
    }
    Some(days_from_civil(y, m, day) * 86_400 + h * 3_600 + min * 60 + sec - offset)
}

/// The time of day and its offset from UTC in seconds: `Z`, `+02:00`, `-0530`.
fn split_offset(rest: &str) -> Option<(&str, i64)> {
    if let Some(time) = rest.strip_suffix(['Z', 'z']) {
        return Some((time, 0));
    }
    let at = rest.rfind(['+', '-'])?;
    let (time, off) = rest.split_at(at);
    let sign = if off.starts_with('-') { -1 } else { 1 };
    let digits: String = off[1..].chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 4 {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let mins: i64 = digits[2..].parse().ok()?;
    Some((time, sign * (hours * 3_600 + mins * 60)))
}

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar (Howard Hinnant's
/// algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[path = "lfs_locks_tests.rs"]
mod tests;
