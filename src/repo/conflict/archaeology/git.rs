//! What the archaeology asks git: each side's commits that touched the file, test merges of two
//! commits (`merge-tree`, which leaves the files and the index as they are) and a commit's
//! change to the file. Blocking: from the background.
//!
//! A merge brings two lines of commits together: each side's first-parent commits since they
//! diverged, merged pairwise as git would merge them. A rebase, cherry-pick or revert applies
//! one commit's change instead (from its parent; a revert's from itself to its parent): ours is
//! the commits of the checked-out side since then, theirs that one commit, and each test merge
//! uses the same base git does.

use std::path::Path;

use crate::git::{self, InProgress};

/// The most commits read for each side; older ones are left out (and said to be).
pub const MAX_COMMITS: usize = 1000;

/// A commit of one side that touched the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub hash: String,
    pub short: String,
    pub author: String,
    pub time: i64,
    pub summary: String,
}

/// Each side's commits that touched the file since they diverged, oldest first, and how two
/// of them are merged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timelines {
    pub ours: Vec<Step>,
    pub theirs: Vec<Step>,
    /// More than [`MAX_COMMITS`] on ours or on theirs: its oldest are left out.
    pub truncated: (bool, bool),
    /// The base every test merge takes (a rebase's, cherry-pick's or revert's); `None`: the two
    /// commits' merge base, as a merge has it.
    pub base: Option<String>,
    /// What is merged in place of theirs (a revert merges the reverted commit's parent).
    pub theirs_rev: Option<String>,
}

impl Timelines {
    /// What a test merge takes for theirs `k`.
    pub fn theirs_merged(&self, k: usize) -> Option<&str> {
        self.theirs_rev
            .as_deref()
            .or_else(|| self.theirs.get(k).map(|s| s.hash.as_str()))
    }
}

/// How an operation merges: the commit it shows for theirs, what it merges in for it, and the
/// base it merges from (`None`: the merge base).
struct Operation {
    shown: String,
    merged: Option<String>,
    base: Option<String>,
}

/// How the operation `state` stopped at merges; or why it can't be looked into.
fn operation(repo: &Path, state: Option<InProgress>) -> Result<Operation, String> {
    let (head, merged, base) = match state {
        Some(InProgress::Merge) => ("MERGE_HEAD", None, None),
        Some(InProgress::Rebase) => ("REBASE_HEAD", None, Some("REBASE_HEAD^")),
        Some(InProgress::CherryPick) => ("CHERRY_PICK_HEAD", None, Some("CHERRY_PICK_HEAD^")),
        // Reverting applies the change from the commit to its parent.
        Some(InProgress::Revert) => ("REVERT_HEAD", Some("REVERT_HEAD^"), Some("REVERT_HEAD")),
        Some(InProgress::Am) | None => return Err(NO_OTHER_SIDE.to_owned()),
    };
    let rev = |name: &str| {
        git::run(Some(repo), &["rev-parse", "--verify", "-q", name])
            .map(|s| s.trim().to_owned())
            .map_err(|_| crate::i18n::message("repo-core-0402", &[("name", format!("{}", name).into())]))
    };
    let shown = rev(head)?;
    let base = match base {
        Some(name) => Some(rev(name).map_err(|_| {
            crate::i18n::message("repo-core-0403", &[("head", format!("{}", head).into())])
        })?),
        None => None,
    };
    Ok(Operation {
        shown,
        merged: merged.map(rev).transpose()?,
        base,
    })
}

/// Said when the conflict comes from no other branch (a stash, a patch).
pub const NO_OTHER_SIDE: &str = crate::i18n::text("repo-core-0404");

const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

/// Parses `log` output in the format [`log`] asks for: each commit with its side's mark
/// (`<`, `>`, or none).
fn parse_steps(out: &str) -> Vec<(char, Step)> {
    out.split(RECORD)
        .filter_map(|record| {
            let mut f = record.trim_start_matches(['\n', '\r']).split(FIELD);
            let mark = f.next()?.chars().next().unwrap_or(' ');
            let hash = f.next()?.trim().to_owned();
            if hash.is_empty() {
                return None;
            }
            let step = Step {
                hash,
                short: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                time: f.next()?.trim().parse().unwrap_or(0),
                summary: f.next().unwrap_or_default().trim_end().to_owned(),
            };
            Some((mark, step))
        })
        .collect()
}

/// The first-parent commits of one side of `range` (`side`: `--left-only` or
/// `--right-only`) that touched `path`, newest first, at most [`MAX_COMMITS`] + 1 (to tell
/// when some are left out). `drop_copies`: leave out those the other side has too, as a
/// rebase, cherry-pick or revert has them replayed; a merge keeps them, as two branches that
/// made the same change still each made it.
fn log(
    repo: &Path,
    range: &str,
    (side, drop_copies): (&str, bool),
    path: &str,
) -> Result<Vec<Step>, String> {
    let count = format!("--max-count={}", MAX_COMMITS + 1);
    let mut args = vec![
        "--literal-pathspecs",
        "log",
        "--no-show-signature",
        "--encoding=UTF-8",
        "--format=%x1e %x1f%H%x1f%h%x1f%an%x1f%at%x1f%s",
        "--first-parent",
        side,
    ];
    if drop_copies {
        args.push("--cherry-pick");
    }
    args.extend([count.as_str(), "--end-of-options", range, "--", path]);
    let out = git::run(Some(repo), &args)?;
    Ok(parse_steps(&out).into_iter().map(|(_, s)| s).collect())
}

/// `steps` (newest first) oldest first, at most [`MAX_COMMITS`] of them, and whether some were
/// left out.
fn oldest_first(mut steps: Vec<Step>) -> (Vec<Step>, bool) {
    let cut = steps.len() > MAX_COMMITS;
    steps.truncate(MAX_COMMITS);
    steps.reverse();
    (steps, cut)
}

/// Each side's commits that touched `path`, for the operation `state` stopped at.
pub fn timelines(repo: &Path, path: &str, state: Option<InProgress>) -> Result<Timelines, String> {
    let op = operation(repo, state)?;
    let Some(base) = op.base else {
        // Each side read on its own, so that a long one leaves the other its share.
        let range = format!("HEAD...{}", op.shown);
        let (ours, cut_ours) = oldest_first(log(repo, &range, ("--left-only", false), path)?);
        let (theirs, cut_theirs) = oldest_first(log(repo, &range, ("--right-only", false), path)?);
        return Ok(Timelines {
            ours,
            theirs,
            truncated: (cut_ours, cut_theirs),
            base: None,
            theirs_rev: None,
        });
    };
    // Ours: what the checked-out side has that the base hasn't (its replayed copies left out).
    let left = log(repo, &format!("HEAD...{base}"), ("--left-only", true), path)?;
    let (ours, cut) = oldest_first(left);
    let theirs = one_commit(repo, &op.shown)?;
    Ok(Timelines {
        ours,
        theirs: vec![theirs],
        truncated: (cut, false),
        base: Some(base),
        theirs_rev: op.merged,
    })
}

/// The commit `hash`, as a step.
fn one_commit(repo: &Path, hash: &str) -> Result<Step, String> {
    let out = git::run(
        Some(repo),
        &[
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%x1e %x1f%H%x1f%h%x1f%an%x1f%at%x1f%s",
            "-1",
            "--end-of-options",
            hash,
        ],
    )?;
    parse_steps(&out)
        .into_iter()
        .next()
        .map(|(_, s)| s)
        .ok_or_else(|| crate::i18n::message("repo-core-0416", &[("hash", format!("{}", hash).into())]))
}

/// Whether merging commits `a` and `b` (from `base`, or their merge base) conflicts in `path`,
/// leaving the files and the index as they are.
pub fn merge_conflicts(
    repo: &Path,
    (a, b): (&str, &str),
    base: Option<&str>,
    path: &str,
) -> Result<bool, String> {
    let from = base.map(|b| format!("--merge-base={b}"));
    let mut args = vec![
        "merge-tree",
        "--write-tree",
        "--name-only",
        "--no-messages",
        "-z",
    ];
    args.extend(from.as_deref());
    args.extend([a, b]);
    let out = git::run_ok_codes(Some(repo), &args, &[0, 1]).map_err(|e| {
        if e.contains("unknown option") || e.contains("usage: git merge-tree") {
            crate::i18n::text("repo-core-0425")
                .to_owned()
        } else {
            crate::i18n::message("repo-core-0426", &[("e", format!("{}", e).into())])
        }
    })?;
    // The tree, then each conflicted file.
    Ok(out.split('\0').skip(1).any(|file| file == path))
}

/// The most lines of a commit's change shown.
const MAX_PATCH_LINES: usize = 400;

/// What commit `hash` changed in `path` (from its first parent, for a merge; at most
/// [`MAX_PATCH_LINES`] lines).
pub fn change(repo: &Path, hash: &str, path: &str) -> Result<String, String> {
    let out = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "show",
            "--no-show-signature",
            "--no-ext-diff",
            "--no-color",
            "--diff-merges=first-parent",
            "--format=",
            "--end-of-options",
            hash,
            "--",
            path,
        ],
    )?;
    let lines: Vec<&str> = out.lines().collect();
    if lines.len() <= MAX_PATCH_LINES {
        return Ok(out);
    }
    let mut kept = lines[..MAX_PATCH_LINES].join("\n");
    kept.push_str(&crate::i18n::message("repo-core-0432", &[("MAX-PATCH-LINES", format!("{}", lines.len() - MAX_PATCH_LINES).into())]));
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_parse_with_odd_summaries() {
        let out = "\x1e<\x1fabc\x1fab\x1fJosé\x1f1700000000\x1ffix: ünïcode | pipes\n\
                   \x1e>\x1fdef\x1fde\x1fA B\x1f12\x1f\n";
        let steps = parse_steps(out);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].0, '<');
        assert_eq!(steps[0].1.author, "José");
        assert_eq!(steps[0].1.summary, "fix: ünïcode | pipes");
        assert_eq!(steps[1].0, '>');
        assert_eq!(steps[1].1.time, 12);
        assert_eq!(steps[1].1.summary, "");
        assert!(parse_steps("").is_empty());
    }

    #[test]
    fn the_oldest_beyond_the_limit_are_left_out() {
        let step = |n: usize| Step {
            hash: n.to_string(),
            short: n.to_string(),
            author: String::new(),
            time: 0,
            summary: String::new(),
        };
        let newest_first: Vec<Step> = (0..MAX_COMMITS + 5).rev().map(step).collect();
        let (kept, cut) = oldest_first(newest_first);
        assert!(cut);
        assert_eq!(kept.len(), MAX_COMMITS);
        assert_eq!(kept.last().map(|s| s.hash.as_str()), Some("1004"));
        assert_eq!(kept.first().map(|s| s.hash.as_str()), Some("5"));
    }
}
