//! What the archaeology asks git: each side's commits that touched the file since the two
//! diverged, test merges of two commits (`merge-tree`, which touches neither the working tree
//! nor the index) and a commit's change to the file. Blocking: from the background.

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

/// Each side's commits that touched the file since they diverged, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timelines {
    pub ours: Vec<Step>,
    pub theirs: Vec<Step>,
    /// More than [`MAX_COMMITS`] on a side: its oldest are left out.
    pub truncated: bool,
}

/// The commit merged in by the operation `state` stopped at, as git names it.
fn other_head(state: Option<InProgress>) -> Result<&'static str, String> {
    match state {
        Some(InProgress::Merge) => Ok("MERGE_HEAD"),
        Some(InProgress::CherryPick) => Ok("CHERRY_PICK_HEAD"),
        Some(InProgress::Revert) => Ok("REVERT_HEAD"),
        Some(InProgress::Rebase) => Ok("REBASE_HEAD"),
        Some(InProgress::Am) => {
            Err("A patch applied with git am has no other branch to look into".to_owned())
        }
        None => Err("No merge, rebase, cherry-pick or revert is in progress".to_owned()),
    }
}

const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

/// Parses `log` output in the format [`side`] asks for.
fn parse_steps(out: &str) -> Vec<Step> {
    out.split(RECORD)
        .filter_map(|record| {
            let mut f = record.trim_start_matches(['\n', '\r']).split(FIELD);
            let hash = f.next()?.trim().to_owned();
            if hash.is_empty() {
                return None;
            }
            Some(Step {
                hash,
                short: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                time: f.next()?.trim().parse().unwrap_or(0),
                summary: f.next().unwrap_or_default().trim_end().to_owned(),
            })
        })
        .collect()
}

/// The commits of one side (`--left-only`: ours, `--right-only`: theirs) of `range` that
/// touched `path`, oldest first, leaving out those the other side has too (as a rebase has
/// them replayed). At most [`MAX_COMMITS`] + 1 are read, to tell when some are left out.
fn side(repo: &Path, range: &str, which: &str, path: &str) -> Result<Vec<Step>, String> {
    let format = "--format=%x1e%H%x1f%h%x1f%an%x1f%at%x1f%s";
    let count = format!("--max-count={}", MAX_COMMITS + 1);
    let out = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            format,
            which,
            "--cherry-pick",
            "--topo-order",
            &count,
            "--end-of-options",
            range,
            "--",
            path,
        ],
    )?;
    let mut steps = parse_steps(&out);
    steps.reverse();
    Ok(steps)
}

/// Each side's commits that touched `path` since the two diverged, for the operation `state`
/// stopped at.
pub fn timelines(repo: &Path, path: &str, state: Option<InProgress>) -> Result<Timelines, String> {
    let other = other_head(state)?;
    git::run(Some(repo), &["rev-parse", "--verify", "-q", other])
        .map_err(|_| format!("{other} is not there: is the operation still in progress?"))?;
    let range = format!("HEAD...{other}");
    let mut ours = side(repo, &range, "--left-only", path)?;
    let mut theirs = side(repo, &range, "--right-only", path)?;
    let truncated = ours.len() > MAX_COMMITS || theirs.len() > MAX_COMMITS;
    for steps in [&mut ours, &mut theirs] {
        if steps.len() > MAX_COMMITS {
            steps.drain(..steps.len() - MAX_COMMITS);
        }
    }
    Ok(Timelines {
        ours,
        theirs,
        truncated,
    })
}

/// Whether merging commits `a` and `b` conflicts in `path`, without touching the working tree
/// or the index.
pub fn merge_conflicts(repo: &Path, a: &str, b: &str, path: &str) -> Result<bool, String> {
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            "-z",
            a,
            b,
        ],
        &[0, 1],
    )
    .map_err(|e| {
        if e.contains("write-tree") || e.contains("usage:") {
            "Finding where it started needs git 2.38 or newer (merge-tree --write-tree)".to_owned()
        } else {
            format!("The test merge failed: {e}")
        }
    })?;
    // The tree, then each conflicted file.
    Ok(out.split('\0').skip(1).any(|file| file == path))
}

/// The most lines of a commit's change shown.
const MAX_PATCH_LINES: usize = 400;

/// What commit `hash` changed in `path` (at most [`MAX_PATCH_LINES`] lines).
pub fn change(repo: &Path, hash: &str, path: &str) -> Result<String, String> {
    let out = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "show",
            "--no-show-signature",
            "--no-ext-diff",
            "--no-color",
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
    kept.push_str(&format!(
        "\n… {} more lines not shown",
        lines.len() - MAX_PATCH_LINES
    ));
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_parse_with_odd_summaries() {
        let out = "\x1eabc\x1fab\x1fJosé\x1f1700000000\x1ffix: ünïcode | pipes\n\
                   \x1edef\x1fde\x1fA B\x1f12\x1f\n";
        let steps = parse_steps(out);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].author, "José");
        assert_eq!(steps[0].summary, "fix: ünïcode | pipes");
        assert_eq!(steps[1].time, 12);
        assert_eq!(steps[1].summary, "");
        assert!(parse_steps("").is_empty());
    }

    #[test]
    fn a_patch_has_no_other_side() {
        assert!(other_head(Some(InProgress::Am)).is_err());
        assert!(other_head(None).is_err());
        assert_eq!(other_head(Some(InProgress::Rebase)), Ok("REBASE_HEAD"));
    }
}
