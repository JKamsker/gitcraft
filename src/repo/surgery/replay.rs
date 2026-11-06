//! A todo list played through without touching anything (`git merge-tree`, one commit after
//! the other, as the rebase would pick them): what tree the branch would end with. A rewrite
//! that should leave the branch's files as they are refuses when it would not, before it runs.

use std::path::Path;

use crate::git;
use crate::repo::rebase::Entry;

/// The tree the rebase of `todo` onto `onto` ends with, or `None` when a pick would conflict
/// (the rebase then stops there, and the usual conflict flow takes over).
pub(crate) fn ends_as(repo: &Path, onto: &str, todo: &[Entry]) -> Result<Option<String>, String> {
    let mut state = git::resolve(repo, onto)?;
    for e in todo {
        let hash = &e.commit.hash;
        let base = format!("--merge-base={hash}^");
        // Exit code 1: conflicts, its first line still the tree.
        let out = git::run_ok_codes(
            Some(repo),
            &[
                "merge-tree",
                "--write-tree",
                "--no-messages",
                &base,
                "--end-of-options",
                &state,
                hash,
            ],
            &[0, 1],
        )
        .map_err(|e| {
            format!("could not play the rewrite through (git 2.40 or newer is needed): {e}")
        })?;
        let mut lines = out.lines();
        let Some(tree) = lines.next().map(str::trim).filter(|t| !t.is_empty()) else {
            return Err("git merge-tree gave no tree".to_owned());
        };
        if lines.any(|l| !l.trim().is_empty()) {
            return Ok(None);
        }
        state = git::commit_tree(repo, tree, &state, "replay", None)?;
    }
    git::tree_of(repo, &state).map(Some)
}
