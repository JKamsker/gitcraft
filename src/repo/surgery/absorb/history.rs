//! The branch with the staged fixes absorbed, made without touching the working tree or the
//! index: each commit from below the oldest commit getting fixes is made again on the one
//! before (`merge-tree`, as a rebase would pick it), its fixes melded in, with its own message
//! and author (and signed when `commit.gpgsign` says so). Absorbing then only moves the branch
//! to the new commits; what is staged and the working tree stay exactly as they are, the
//! absorbed changes being in HEAD now.

use std::collections::HashMap;
use std::path::Path;

use crate::git;
use crate::repo::short;

/// The tree of `ours` and `theirs` (commits) merged from `base`, `None` when they conflict.
fn merge(repo: &Path, base: &str, ours: &str, theirs: &str) -> Result<Option<String>, String> {
    let base = format!("--merge-base={base}");
    // Exit code 1: conflicts, the first line still the tree.
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "merge-tree",
            "--write-tree",
            "--no-messages",
            &base,
            "--end-of-options",
            ours,
            theirs,
        ],
        &[0, 1],
    )
    .map_err(|e| {
        crate::i18n::message(
            "repo-workflows-surgery-merge-7ec9d56b",
            &[("e", (e).clone().into())],
        )
    })?;
    let mut lines = out.lines();
    let Some(tree) = lines.next().map(str::trim).filter(|t| !t.is_empty()) else {
        return Err(crate::i18n::text("repo-workflows-surgery-merge-810c20dd").to_owned());
    };
    Ok((!lines.any(|l| !l.trim().is_empty())).then(|| tree.to_owned()))
}

/// The commits of the branch from `oldest` up to `head`, oldest first.
fn commits_from(repo: &Path, oldest: &str, head: &str) -> Result<Vec<String>, String> {
    let range = format!("{oldest}^..{head}");
    Ok(git::run(
        Some(repo),
        &[
            "rev-list",
            "--reverse",
            "--first-parent",
            "--end-of-options",
            &range,
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect())
}

/// The whole message of `commit`, byte for byte: what follows its headers.
fn message_of(repo: &Path, commit: &str) -> Result<String, String> {
    let raw = git::run(Some(repo), &["cat-file", "commit", commit])?;
    Ok(raw
        .split_once("\n\n")
        .map(|(_, message)| message.to_owned())
        .unwrap_or_default())
}

/// The new HEAD: the commits from below the oldest of `fixes` (commit → tree of `head` with
/// its fixes) made again with them. Refused, with why, when a fix conflicts with a later commit
/// or the branch would not end with `absorbed` (`head` with every fix).
pub(crate) fn absorbed(
    repo: &Path,
    head: &str,
    oldest: &str,
    fixes: &HashMap<String, String>,
    absorbed: &str,
) -> Result<String, String> {
    let sign = git::run(Some(repo), &["config", "--bool", "commit.gpgsign"])
        .is_ok_and(|v| v.trim() == "true");
    let mut state = git::resolve(repo, &format!("{oldest}^"))?;
    for commit in commits_from(repo, oldest, head)? {
        let parent = format!("{commit}^");
        let Some(mut tree) = merge(repo, &parent, &state, &commit)? else {
            return Err(crate::i18n::message(
                "repo-workflows-surgery-absorbed-4b7f1a77",
                &[("short-commit-1", (short(&commit)).clone().into())],
            ));
        };
        if let Some(fixed) = fixes.get(&commit) {
            // The fix as a commit on HEAD, melded into what the commit is now.
            let ours = git::commit_tree(repo, &tree, &state, "absorb", None)?;
            let fix = git::commit_tree(repo, fixed, head, "fixup", None)?;
            tree = merge(repo, head, &ours, &fix)?.ok_or_else(|| {
                crate::i18n::message(
                    "repo-workflows-surgery-absorbed-0ecdae65",
                    &[("short-commit-1", (short(&commit)).clone().into())],
                )
            })?;
        }
        let author = git::author_of(repo, &commit)?;
        let message = message_of(repo, &commit)?;
        state = git::commit_tree_signed(repo, &tree, &state, &message, Some(&author), sign)?;
    }
    if git::tree_of(repo, &state)? != absorbed {
        return Err(crate::i18n::text("repo-workflows-surgery-absorbed-d6840076").to_owned());
    }
    Ok(state)
}
