//! What the timeline finds out in the background about an entry: the files it changed, and for
//! a change made elsewhere, what it was, from the reflog ("Commit “Fix the parser”",
//! "Rebase", "Reset to HEAD~1").

use std::path::Path;

use super::{Entry, MAX_FILES, RefChange};
use crate::git;

/// Reflog entries read back at most, looking for where a ref was when last seen.
const REFLOG_DEPTH: usize = 20;

/// What the background work found for an entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) files: Vec<String>,
    pub(crate) files_total: usize,
    /// A better name for a change made elsewhere, when the reflog says what it was.
    pub(crate) label: Option<String>,
    /// What it did to a branch it rewrote.
    pub(crate) receipt: Option<Result<super::receipt::Receipt, String>>,
}

/// The two commits whose difference is what `entry` did to the files: HEAD's when it moved,
/// else the first branch that moved from one commit to another.
pub(crate) fn moved_commits(entry: &Entry) -> Option<(String, String)> {
    let (b, a) = (&entry.before.head, &entry.after.head);
    if !b.is_empty() && !a.is_empty() && b != a {
        return Some((b.clone(), a.clone()));
    }
    entry
        .changes()
        .into_iter()
        .find(|c| c.name != "HEAD" && !c.old.is_empty() && !c.new.is_empty())
        .map(|c| (c.old, c.new))
}

/// The files that differ between `from` and `to` (at most [`MAX_FILES`]), and how many in all.
pub(crate) fn files_between(
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<(Vec<String>, usize), String> {
    let out = git::run(
        Some(repo),
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            from,
            to,
            "--",
        ],
    )?;
    let all: Vec<&str> = out.split('\0').filter(|s| !s.is_empty()).collect();
    let total = all.len();
    Ok((
        all.into_iter().take(MAX_FILES).map(str::to_owned).collect(),
        total,
    ))
}

/// Finds out what is to know about `entry`: its files, and for a change made elsewhere its name.
pub(crate) fn find(repo: &Path, entry: &Entry) -> Found {
    let mut found = Found::default();
    if let Some((from, to)) = moved_commits(entry)
        && let Ok((files, total)) = files_between(repo, &from, &to)
    {
        found.files = files;
        found.files_total = total;
    }
    if let Some((branch, old, new)) = super::receipt::rewritten(repo, entry) {
        found.receipt = Some(super::receipt::receipt(repo, &branch, &old, &new));
    }
    if entry.origin == super::Origin::Outside {
        found.label = Some(outside_label(repo, entry));
    } else if matches!(entry.label.as_str(), "Commit" | "Amend") && !entry.after.head.is_empty() {
        // Which commit: by its message.
        found.label = git::run(
            Some(repo),
            &[
                "log",
                "-1",
                "--no-show-signature",
                "--format=%s",
                &entry.after.head,
                "--",
            ],
        )
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .map(|s| format!("{} “{}”", entry.label, clip(&s, 80)));
    }
    found
}

/// `s` cut to `max` characters, with an ellipsis when cut.
fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", &s[..at]),
        None => s.to_owned(),
    }
}

/// What a change made elsewhere was: what the reflog of HEAD (or of the branch that moved) says
/// was done since it was last seen, else which branches moved.
fn outside_label(repo: &Path, entry: &Entry) -> String {
    let changes = entry.changes();
    let head_moved =
        entry.before.head != entry.after.head || entry.before.branch != entry.after.branch;
    let (log, since) = if head_moved {
        ("HEAD".to_owned(), entry.before.head.clone())
    } else {
        match changes
            .iter()
            .find(|c| c.name != "HEAD" && !c.new.is_empty())
        {
            Some(c) => (format!("refs/heads/{}", c.name), c.old.clone()),
            None => return fallback_label(&changes),
        }
    };
    // HEAD on another branch at the same commit (a checkout): the newest entry says so.
    let branch_only = entry.before.head == entry.after.head;
    let subjects = reflog_since(repo, &log, &since, branch_only);
    if subjects.is_empty() {
        return fallback_label(&changes);
    }
    // Oldest first.
    let names: Vec<String> = subjects.iter().rev().map(|s| describe_reflog(s)).collect();
    let mut deduped: Vec<String> = Vec::new();
    for name in names {
        if deduped.last() != Some(&name) {
            deduped.push(name);
        }
    }
    match deduped.as_slice() {
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first}, and {} more", rest.len()),
        [] => fallback_label(&changes),
    }
}

/// The subjects of `log`'s reflog, newest first, back to the entry that left it at `since`
/// (not included); none when it can't be read, or `since` isn't found within
/// [`REFLOG_DEPTH`] entries. With `at_least_one`, the newest is taken even when it left the
/// ref at `since` (a checkout of another branch at the same commit).
pub(crate) fn reflog_since(repo: &Path, log: &str, since: &str, at_least_one: bool) -> Vec<String> {
    let depth = format!("-n{REFLOG_DEPTH}");
    let Ok(out) = git::run(
        Some(repo),
        &[
            "log",
            "-g",
            "--no-show-signature",
            &depth,
            "--format=%H%x1f%gs",
            log,
            "--",
        ],
    ) else {
        return Vec::new();
    };
    parse_reflog(&out, since, at_least_one)
}

/// [`reflog_since`]'s reading of `log -g --format=%H%x1f%gs`.
pub(crate) fn parse_reflog(out: &str, since: &str, at_least_one: bool) -> Vec<String> {
    let mut subjects = Vec::new();
    for line in out.lines() {
        let Some((commit, subject)) = line.split_once('\x1f') else {
            continue;
        };
        if commit == since {
            if at_least_one && subjects.is_empty() {
                subjects.push(subject.to_owned());
            }
            return subjects;
        }
        subjects.push(subject.to_owned());
    }
    // Not found: whatever is there is older than what was seen as well.
    if since.is_empty() {
        subjects
    } else {
        Vec::new()
    }
}

/// A reflog subject the way the timeline names it.
pub(crate) fn describe_reflog(subject: &str) -> String {
    let (what, rest) = subject.split_once(": ").unwrap_or((subject, ""));
    let rest = rest.trim();
    let quoted = |prefix: &str| format!("{prefix} “{rest}”");
    match what {
        "commit" | "commit (initial)" => quoted("Commit"),
        "commit (amend)" => quoted("Amend"),
        "commit (merge)" => quoted("Merge commit"),
        "cherry-pick" => quoted("Cherry-pick"),
        "revert" => quoted("Revert"),
        w if w.starts_with("rebase") => "Rebase".to_owned(),
        "reset" => match rest.strip_prefix("moving to ") {
            Some(to) => format!("Reset to {to}"),
            None => "Reset".to_owned(),
        },
        "checkout" => match rest.rsplit_once(" to ") {
            Some((_, to)) => format!("Checkout {to}"),
            None => "Checkout".to_owned(),
        },
        "pull" => "Pull".to_owned(),
        "branch" => "Branch made".to_owned(),
        w if w.starts_with("merge ") => format!("Merge {}", &w["merge ".len()..]),
        w if w.starts_with("pull") => "Pull".to_owned(),
        _ if subject.trim().is_empty() => "Moved".to_owned(),
        _ => subject.trim().to_owned(),
    }
}

/// When the reflog says nothing: which branches were made, moved or deleted.
pub(crate) fn fallback_label(changes: &[RefChange]) -> String {
    let branches: Vec<&RefChange> = changes.iter().filter(|c| c.name != "HEAD").collect();
    let one = |c: &RefChange| {
        if c.old.is_empty() {
            format!("Branch {} made", c.name)
        } else if c.new.is_empty() {
            format!("Branch {} deleted", c.name)
        } else {
            format!("{} moved", c.name)
        }
    };
    match branches.as_slice() {
        [] => match changes.first() {
            Some(head) => format!("Checkout {}", named(&head.new)),
            None => "Moved".to_owned(),
        },
        [c] => one(c),
        [c, rest @ ..] => format!("{}, and {} more", one(c), rest.len()),
    }
}

/// A branch by its name, a commit by its short hash.
fn named(value: &str) -> &str {
    if value.len() >= 40 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        crate::repo::short(value)
    } else {
        value
    }
}
