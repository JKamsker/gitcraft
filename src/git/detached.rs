//! A detached HEAD: the branches it was on before, whether a bisect is what detached it, and
//! the commits made on it that no branch, tag or remote branch has.

use std::path::Path;

use super::{NO_SIGNATURE, UTF8, run};

/// What the banner of a detached HEAD needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetachedInfo {
    /// The branches HEAD was on before, the latest first (from its reflog): the one to go back
    /// to is the first that still exists.
    pub previous: Vec<String>,
    /// A bisect is going on: it detached HEAD, and has its own banner.
    pub bisecting: bool,
}

/// How many of HEAD's reflog entries are looked through for the branch it was on.
const REFLOG_LOOKED_AT: &str = "-n300";

pub fn detached_info(repo: &Path) -> DetachedInfo {
    let reflog = run(
        Some(repo),
        &[
            "log",
            "-g",
            NO_SIGNATURE,
            UTF8,
            "--format=%gs",
            REFLOG_LOOKED_AT,
            "HEAD",
        ],
    )
    .unwrap_or_default();
    DetachedInfo {
        previous: previous_branches(&reflog),
        // It says "We are not bisecting." and fails otherwise.
        bisecting: run(Some(repo), &["bisect", "log"]).is_ok(),
    }
}

/// The branches HEAD's reflog subjects (`reflog`, the latest first) say it was on, each once:
/// `checkout: moving from main to 1a2b3c4` was on `main`. A branch name has no spaces, so the
/// words are where the spaces are. A detached HEAD's commit is in the list too: which of them
/// are branches is up to the branches there are.
fn previous_branches(reflog: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for subject in reflog.lines() {
        let from = subject
            .strip_prefix("checkout: moving from ")
            .and_then(|rest| rest.split(' ').next());
        let returned = subject
            .strip_prefix("rebase (finish): returning to refs/heads/")
            .or_else(|| subject.strip_prefix("rebase -i (finish): returning to refs/heads/"));
        let Some(name) = from.or(returned).map(str::trim) else {
            continue;
        };
        if !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    names
}

/// A commit made on a detached HEAD: (hash, summary).
pub type LooseCommit = (String, String);

/// At most this many are listed; more are said to be more.
pub const LOOSE_LISTED: usize = 100;

/// The commits of HEAD that checking out `target` would leave on no branch: in no branch, tag
/// or remote branch, nor in `target` itself. The latest first, at most [`LOOSE_LISTED`] + 1.
pub fn left_behind(repo: &Path, target: &str) -> Result<Vec<LooseCommit>, String> {
    let max = format!("--max-count={}", LOOSE_LISTED + 1);
    // `^…` can't be read as an option, whatever the name.
    let not_target = format!("^{target}");
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--format=%H%x00%s",
            &max,
            "HEAD",
            &not_target,
            "--not",
            "--branches",
            "--tags",
            "--remotes",
            "--",
        ],
    )?;
    Ok(out
        .lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(hash, summary)| (hash.to_owned(), summary.to_owned()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branches_left_are_read_from_the_reflog_latest_first() {
        let reflog = "\
checkout: moving from 1a2b3c4d to 9f8e7d6c
commit: wip
checkout: moving from feature/ünï to 1a2b3c4d
rebase (finish): returning to refs/heads/topic
checkout: moving from main to feature/ünï
checkout: moving from feature/ünï to main
reset: moving to HEAD~1";
        assert_eq!(
            previous_branches(reflog),
            ["1a2b3c4d", "feature/ünï", "topic", "main"]
        );
        assert!(previous_branches("").is_empty());
    }
}
