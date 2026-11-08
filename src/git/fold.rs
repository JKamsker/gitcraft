//! What folding the staged changes into an older commit needs to know, read in one go: the
//! commit, the ones after it that get rewritten, which of them are pushed, and whether there is
//! anything staged to fold in.

use std::collections::HashSet;
use std::path::Path;

use super::{InProgress, RebaseCommit, in_progress, not_an_option, run, run_ok_codes};

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
    /// A shallow clone: a commit without parents may only be where its history was cut.
    pub shallow: bool,
    /// An ignored file (or folder) where a replayed commit has a file: checking that commit out
    /// would overwrite it, which git does without asking for ignored files.
    pub ignored_in_way: Option<String>,
}

/// The ignored file or folder of `ignored` (`--directory` listing: folders end with `/`) that
/// one of the `touched` paths would be written over, if any.
fn in_the_way(ignored: &str, touched: &str) -> Option<String> {
    let ignored: Vec<&str> = ignored.split('\0').filter(|p| !p.is_empty()).collect();
    let hit = |path: &str| {
        ignored
            .iter()
            .find(|i| match i.strip_suffix('/') {
                Some(dir) => path == dir || path.starts_with(**i),
                None => path == **i,
            })
            .map(|i| (*i).to_owned())
    };
    // Names as they are, spaces and all. Should a git version put a line break between
    // commits, the name without it is tried too: a name can start with one as well.
    touched
        .split('\0')
        .filter(|p| !p.is_empty())
        .find_map(|path| hit(path).or_else(|| path.strip_prefix('\n').and_then(hit)))
}

/// The branch the rebase going on was started on (`refs/heads/main`), if one is going on.
pub fn rebasing_branch(repo: &Path) -> Result<Option<String>, String> {
    let printed = run(
        Some(repo),
        &["rev-parse", "--git-path", "rebase-merge/head-name"],
    )?;
    let path = crate::remote::resolve(repo, printed.trim());
    match crate::remote::read_to_string(&path) {
        Ok(name) => Ok(Some(name.trim().to_owned())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::i18n::message(
            "backend-could-not-read-the-rebase-s-state-e-367726",
            &[("detail", (e).to_string().into())],
        )),
    }
}

/// Reads what folding the staged changes into `target` would rewrite.
pub fn fold_read(repo: &Path, target: &str) -> Result<FoldRead, String> {
    not_an_option(target)?;
    // From here on a full hash, which no git command reads as an option.
    let target = super::resolve(repo, target)?;
    let head = super::head(repo);
    if head.is_empty() {
        return Err(crate::i18n::text("backend-the-branch-has-no-commits-yet-0733a9").to_owned());
    }
    // Not `--short`, which says `heads/x` when a tag is called `x` too.
    let branch = run(Some(repo), &["symbolic-ref", "-q", "HEAD"])
        .ok()
        .and_then(|b| b.trim().strip_prefix("refs/heads/").map(str::to_owned))
        .filter(|b| !b.is_empty());
    let parents = run(Some(repo), &["rev-list", "--parents", "-n", "1", &target])?;
    let root = parents.split_whitespace().count() == 1;
    let shallow = root
        && run(Some(repo), &["rev-parse", "--is-shallow-repository"])
            .is_ok_and(|s| s.trim() == "true");
    // Exit 1 with nothing printed: no common ancestor. Anything else (a machine not reached)
    // is an error, not an answer.
    let base = run_ok_codes(Some(repo), &["merge-base", &target, &head], &[0, 1])?;
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
    let ignored_in_way = if ancestor {
        let ignored = run(
            Some(repo),
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )?;
        if ignored.is_empty() {
            None
        } else {
            // Without renames: a renamed file's old name is written too (checking out the
            // commit before the rename).
            let touched = run(
                Some(repo),
                &[
                    "log",
                    "--format=",
                    "--name-only",
                    "--no-renames",
                    "-z",
                    &range,
                ],
            )?;
            in_the_way(&ignored, &touched)
        }
    } else {
        None
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
        shallow,
        ignored_in_way,
    })
}

#[cfg(test)]
mod tests {
    use super::in_the_way;

    #[test]
    fn ignored_files_a_commit_would_write_over() {
        let ignored = "build/\0notes.txt\0";
        assert_eq!(
            in_the_way(ignored, "src/a.rs\0build/out.o\0"),
            Some("build/".into())
        );
        assert_eq!(
            in_the_way(ignored, "\nnotes.txt\0"),
            Some("notes.txt".into())
        );
        assert_eq!(in_the_way(ignored, "notes.txt.bak\0buildx/y\0"), None);
        // Spaces are part of a name.
        assert_eq!(in_the_way(" x \0", "x\0 x \0"), Some(" x ".into()));
        assert_eq!(in_the_way("x\0", " x \0"), None);
        // A line break at the start: a name's own, or one between commits.
        assert_eq!(in_the_way("\nx\0", "a\0\nx\0"), Some("\nx".into()));
        assert_eq!(in_the_way("x\0", "a\0\nx\0"), Some("x".into()));
        assert_eq!(in_the_way("\nx\0", "a\0\n\nx\0"), Some("\nx".into()));
        assert_eq!(in_the_way("", "a\0"), None);
    }
}
