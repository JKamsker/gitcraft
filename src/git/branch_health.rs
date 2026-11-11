//! What the branch cleanup and the sort by last commit read about the local branches: when each
//! last had a commit, which are merged into a base, and which have all their work in it under
//! other hashes (rebased or cherry-picked).

use std::collections::HashSet;
use std::path::Path;

use super::{FS, run};

/// The local branches, newest last commit first: `(name, committer time in seconds)`.
pub fn branches_by_date(repo: &Path) -> Result<Vec<(String, i64)>, String> {
    let format = format!("--format=%(refname){FS}%(committerdate:unix)");
    let out = run(
        Some(repo),
        &[
            "for-each-ref",
            "--sort=-committerdate",
            &format,
            "refs/heads",
        ],
    )?;
    Ok(parse_dates(&out))
}

/// `for-each-ref` lines of `<full ref><FS><unix time>`; a ref with no date (a tag object
/// somewhere odd) counts as made at 0.
fn parse_dates(out: &str) -> Vec<(String, i64)> {
    out.lines()
        .filter_map(|line| {
            let (full, time) = line.split_once(FS)?;
            let name = full.strip_prefix("refs/heads/")?;
            Some((name.to_owned(), time.trim().parse().unwrap_or(0)))
        })
        .collect()
}

/// The local branches whose tip is in `base` (a full ref, `refs/remotes/origin/main`), as
/// `git branch --merged <base>` lists them.
pub fn merged_into(repo: &Path, base: &str) -> Result<HashSet<String>, String> {
    let merged = format!("--merged={base}");
    let out = run(
        Some(repo),
        &["for-each-ref", &merged, "--format=%(refname)", "refs/heads"],
    )?;
    Ok(out
        .lines()
        .filter_map(|l| l.strip_prefix("refs/heads/"))
        .map(str::to_owned)
        .collect())
}

/// Whether every commit `branch` has that `base` hasn't is in `base` all the same, as a commit
/// with the same changes (`git cherry` marks those `-`): the branch was rebased or
/// cherry-picked into it. A branch with no such commits at all is merged, not this.
pub fn cherry_equivalent(repo: &Path, base: &str, branch: &str) -> Result<bool, String> {
    let out = run(Some(repo), &["cherry", base, branch])?;
    Ok(all_upstream(&out))
}

/// `git cherry` output: some commits, and each one `-` (its change is upstream already).
fn all_upstream(out: &str) -> bool {
    let mut lines = out.lines().filter(|l| !l.trim().is_empty()).peekable();
    lines.peek().is_some() && lines.all(|l| l.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_read_by_branch() {
        let out = format!(
            "refs/heads/main{FS}1700000000\nrefs/heads/fix/ünï cödé{FS}1600000000\n\
             refs/heads/odd{FS}\nnot a ref line\n"
        );
        assert_eq!(
            parse_dates(&out),
            vec![
                ("main".to_owned(), 1_700_000_000),
                ("fix/ünï cödé".to_owned(), 1_600_000_000),
                ("odd".to_owned(), 0),
            ]
        );
        assert!(parse_dates("").is_empty());
    }

    #[test]
    fn cherry_output_is_upstream_only_when_every_commit_is() {
        assert!(all_upstream("- 1111\n- 2222\n"));
        assert!(!all_upstream("- 1111\n+ 2222\n"));
        assert!(!all_upstream("+ 1111\n"));
        assert!(!all_upstream(""));
        assert!(!all_upstream("\n\n"));
    }
}
