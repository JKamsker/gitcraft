//! What folding the staged changes into an older commit needs to know, read in one go: the
//! commit, the ones after it that get rewritten, which of them are pushed, and whether there is
//! anything staged to fold in.

use std::collections::HashSet;
use std::path::Path;

use super::{InProgress, RebaseCommit, in_progress, not_an_option, run};

/// The repository as a fold into `target` would find it.
#[derive(Clone, Debug, Default)]
pub struct FoldRead {
    /// The commit folded into and those after it up to HEAD, oldest first (the target first
    /// when it is an ancestor of HEAD on a line without merges).
    pub commits: Vec<RebaseCommit>,
    /// The commit asked for, in full.
    pub target: String,
    /// The target has no parent: the rebase starts from the root.
    pub root: bool,
    /// The target is an ancestor of HEAD.
    pub ancestor: bool,
    /// HEAD as it was read.
    pub head: String,
    /// The branch checked out (`main`), `None` when HEAD is detached.
    pub branch: Option<String>,
    /// The commits of `commits` that no remote-tracking branch has.
    pub unpushed: HashSet<String>,
    /// Number of files staged.
    pub staged: usize,
    pub in_progress: Option<InProgress>,
}

/// Reads what folding the staged changes into `target` would rewrite.
pub fn fold_read(repo: &Path, target: &str) -> Result<FoldRead, String> {
    not_an_option(target)?;
    // From here on a full hash, which no git command reads as an option.
    let target = super::resolve(repo, target)?;
    let head = super::head(repo);
    if head.is_empty() {
        return Err("the branch has no commits yet".to_owned());
    }
    // Not `--short`, which says `heads/x` when a tag is called `x` too.
    let branch = run(Some(repo), &["symbolic-ref", "-q", "HEAD"])
        .ok()
        .and_then(|b| b.trim().strip_prefix("refs/heads/").map(str::to_owned))
        .filter(|b| !b.is_empty());
    let parents = run(Some(repo), &["rev-list", "--parents", "-n", "1", &target])?;
    let root = parents.split_whitespace().count() == 1;
    let base = run(Some(repo), &["merge-base", &target, &head]).unwrap_or_default();
    let ancestor = base.trim() == target;
    // The commits a rebase from below the target replays: all of them for the root.
    let range = if root {
        "HEAD".to_owned()
    } else {
        format!("{target}^..HEAD")
    };
    let commits = if ancestor {
        super::rebase_commits_in(repo, &range)?
    } else {
        Vec::new()
    };
    let unpushed = if ancestor {
        run(Some(repo), &["rev-list", &range, "--not", "--remotes"])?
            .lines()
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        HashSet::new()
    };
    let staged = run(Some(repo), &["diff", "--cached", "--name-only", "-z"])?
        .split('\0')
        .filter(|p| !p.is_empty())
        .count();
    Ok(FoldRead {
        commits,
        target,
        root,
        ancestor,
        head,
        branch,
        unpushed,
        staged,
        in_progress: in_progress(repo)?,
    })
}
