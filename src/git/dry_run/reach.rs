//! Merges worked out in memory, and which commits a change of refs leaves on no ref.

use std::path::Path;

use super::super::{END_OPTS, run, run_ok_codes};
use super::picks::parse_merge_tree;
use super::{Engine, GitVersion, commit_oid, is_oid, scratch};

/// A merge worked out without making it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    /// The merged tree (with conflict markers in the files in conflict). Empty when the
    /// scratch clone stopped on conflicts.
    pub tree: String,
    /// The files in conflict; none for a clean merge.
    pub conflicts: Vec<String>,
    pub engine: Engine,
}

/// Merging `theirs` into `ours`, as `git merge` would (both commits).
pub fn merge(repo: &Path, ours: &str, theirs: &str, version: GitVersion) -> Result<Merge, String> {
    // Names may mean something else (or nothing) in a scratch clone: their commits don't.
    let (ours, theirs) = (&commit_oid(repo, ours)?, &commit_oid(repo, theirs)?);
    super::not_partial(repo)?;
    if !version.has_merge_tree() {
        return scratch::merge(repo, ours, theirs);
    }
    let out = run_ok_codes(
        Some(repo),
        &[
            "merge-tree",
            "--write-tree",
            "-z",
            "--name-only",
            ours,
            theirs,
        ],
        &[0, 1],
    )?;
    let (tree, conflicts) = parse_merge_tree(&out)?;
    Ok(Merge {
        tree,
        conflicts,
        engine: Engine::MergeTree,
    })
}

/// `name` as an `--exclude` pattern matching only itself: glob characters escaped (git
/// refuses them in a branch name, but a ref made by other means may have one).
fn literal_pattern(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '*' | '?' | '[' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The commits that are on no ref once the refs `moved` (full names, `HEAD` when it moves
/// too) go from the commits `old` to `new` (`git rev-list old --not new`, and not the other
/// refs either), newest first, at most `limit`. Stashes and other worktrees' checkouts keep
/// their commits.
pub fn unreachable(
    repo: &Path,
    old: &[String],
    new: &[String],
    moved: &[String],
    limit: usize,
) -> Result<Vec<String>, String> {
    let args = unreachable_args(old, new, moved, limit)?;
    if old.is_empty() {
        return Ok(Vec::new());
    }
    Ok(run(
        Some(repo),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )?
    .lines()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .collect())
}

fn unreachable_args(
    old: &[String],
    new: &[String],
    moved: &[String],
    limit: usize,
) -> Result<Vec<String>, String> {
    if let Some(bad) = old.iter().chain(new).find(|h| !is_oid(h)) {
        return Err(format!("{bad} is not a commit id"));
    }
    if old.is_empty() {
        return Ok(Vec::new());
    }
    let max = format!("--max-count={limit}");
    let excludes: Vec<String> = moved
        .iter()
        .map(|r| format!("--exclude={}", literal_pattern(r)))
        .collect();
    let negated: Vec<String> = new.iter().map(|h| format!("^{h}")).collect();
    let mut args = vec!["rev-list", &max, "--not"];
    args.extend(excludes.iter().map(String::as_str));
    args.extend(["--all", "--not", END_OPTS]);
    args.extend(old.iter().map(String::as_str));
    args.extend(negated.iter().map(String::as_str));
    Ok(args.into_iter().map(str::to_owned).collect())
}

pub(crate) fn unreachable_bounded(
    reader: &super::super::investigations::Reader<'_>,
    old: &[String],
    new: &[String],
    moved: &[String],
    limit: usize,
) -> Result<Vec<String>, String> {
    let args = unreachable_args(old, new, moved, limit)?;
    if args.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = reader.run(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        vec![],
        &[0],
    )?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid local reachability output")?;
    let hashes: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if hashes.len() > limit || hashes.iter().any(|s| !is_oid(s)) {
        return Err("Invalid local reachability output".into());
    }
    Ok(hashes)
}

/// The commits `old` has that `new` hasn't (`git rev-list old --not new`), newest first, at
/// most `limit`: what leaves a branch that moves from `old` to `new`, wherever else they are.
pub fn leaving(repo: &Path, old: &str, new: &str, limit: usize) -> Result<Vec<String>, String> {
    if !is_oid(old) || !is_oid(new) {
        return Err(format!("{old} or {new} is not a commit id"));
    }
    let max = format!("--max-count={limit}");
    Ok(run(
        Some(repo),
        &["rev-list", &max, END_OPTS, old, &format!("^{new}")],
    )?
    .lines()
    .map(|l| l.trim().to_owned())
    .filter(|l| !l.is_empty())
    .collect())
}

/// Whether `ancestor` is `descendant` or one of its ancestors.
pub fn is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> bool {
    !ancestor.starts_with('-')
        && !descendant.starts_with('-')
        && run(
            Some(repo),
            &["merge-base", "--is-ancestor", ancestor, descendant],
        )
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::literal_pattern;

    #[test]
    fn a_branch_name_excludes_only_itself() {
        assert_eq!(literal_pattern("refs/heads/feat"), "refs/heads/feat");
        assert_eq!(
            literal_pattern("refs/heads/a*[b]?"),
            "refs/heads/a\\*\\[b]\\?"
        );
    }
}
