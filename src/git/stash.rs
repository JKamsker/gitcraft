//! Stashes that git's own `stash push` doesn't make: a snapshot, which saves the changes as a
//! stash and leaves the working tree and the index exactly as they are; and whether this git
//! can stash only what is staged.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::{LITERAL_PATHS, NO_SIGNATURE, UTF8, head, run, run_with_index};

/// The first git with `git stash push --staged`.
pub const STAGED_STASH_SINCE: (u32, u32) = (2, 35);

/// The `(major, minor)` of a `git --version` line: `git version 2.43.0`,
/// `git version 2.35.1.windows.2`, `git version 2.39.3 (Apple Git-145)`.
pub fn parse_version(line: &str) -> Option<(u32, u32)> {
    let number = line.trim().strip_prefix("git version ")?;
    let mut parts = number.split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Whether the git of the machine `repo` is on can stash only the staged changes; why not
/// when it can't, as the dialog says it.
pub fn staged_stash_support(repo: &Path) -> Result<(), String> {
    let line = run(Some(repo), &["--version"])
        .map_err(|e| format!("Couldn't ask git for its version: {e}"))?;
    match parse_version(&line) {
        Some(v) if v >= STAGED_STASH_SINCE => Ok(()),
        Some((major, minor)) => Err(format!(
            "Needs git {}.{} or newer; this repository's git is {major}.{minor}",
            STAGED_STASH_SINCE.0, STAGED_STASH_SINCE.1
        )),
        None => Err(format!(
            "Couldn't tell which git this is (it says “{}”)",
            line.trim()
        )),
    }
}

/// A snapshot made and ready to be stored: `git stash store -m <message> <commit>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub commit: String,
    pub message: String,
}

/// Untracked files go to `git add` this many at a time, or fewer when their names are long, so
/// that no command line gets too long (Windows allows 32 767 characters).
const ADD_CHUNK: usize = 400;
const ADD_CHUNK_BYTES: usize = 16 * 1024;

/// Makes a stash commit of the changes as they are, untracked files too when `untracked`,
/// without touching the working tree or the index: git works on a copy of the index, and the
/// untracked files go into an index of their own. `None` when there is nothing to save.
/// `message` (optional) names it as `git stash push -m` would.
pub fn snapshot(repo: &Path, message: &str, untracked: bool) -> Result<Option<Snapshot>, String> {
    let base = head(repo);
    if base.is_empty() {
        return Err("There is no commit yet to save a snapshot on: make the first commit".into());
    }
    let scratch = Scratch::new(repo)?;
    let on = describe_head(repo)?;
    let tracked = run_with_index(repo, &scratch.copy, &["stash", "create"])
        .map_err(|e| format!("git couldn't save the changes: {e}"))?;
    let tracked = tracked.trim();
    let untracked = if untracked {
        untracked_commit(repo, &scratch, &on)?
    } else {
        None
    };
    if tracked.is_empty() && untracked.is_none() {
        return Ok(None);
    }
    // The index's commit: from `stash create`, or HEAD's tree when only new files changed.
    let (tree, index) = if tracked.is_empty() {
        let tree = format!("{base}^{{tree}}");
        let index = commit_tree(repo, &tree, &[&base], &format!("index on {on}"))?;
        (tree, index)
    } else {
        (
            format!("{tracked}^{{tree}}"),
            rev(repo, &format!("{tracked}^2"))?,
        )
    };
    let message = stash_message(&on, message);
    let mut parents = vec![base.as_str(), index.as_str()];
    if let Some(u) = &untracked {
        parents.push(u);
    }
    let commit = commit_tree(repo, &tree, &parents, &message)?;
    Ok(Some(Snapshot { commit, message }))
}

/// What a stash is called, the way `git stash push` calls it: `On main: <message>`, or
/// `WIP on main: 1234567 Last commit` without one.
pub fn stash_message(on: &str, message: &str) -> String {
    let message = message.trim();
    if message.is_empty() {
        format!("WIP on {on}")
    } else {
        let branch = on.split_once(": ").map_or(on, |(b, _)| b);
        format!("On {branch}: {message}")
    }
}

/// `main: 1234567 Last commit` (`(no branch)` when detached), as stash messages say it.
fn describe_head(repo: &Path) -> Result<String, String> {
    let branch = super::run_ok_codes(
        Some(repo),
        &["symbolic-ref", "--short", "-q", "HEAD"],
        &[0, 1],
    )?;
    let branch = branch.trim();
    let branch = if branch.is_empty() {
        "(no branch)"
    } else {
        branch
    };
    let commit = run(
        Some(repo),
        &["log", "-1", UTF8, NO_SIGNATURE, "--format=%h %s", "HEAD"],
    )?;
    Ok(format!("{branch}: {}", commit.trim()))
}

/// The commit of the untracked files (none of them ignored), with no parent, as `git stash
/// push --include-untracked` makes it; `None` when there are none.
fn untracked_commit(repo: &Path, scratch: &Scratch, on: &str) -> Result<Option<String>, String> {
    let listed = run_with_index(
        repo,
        &scratch.copy,
        &["ls-files", "-z", "--others", "--exclude-standard"],
    )?;
    let files: Vec<&str> = listed.split('\0').filter(|f| !f.is_empty()).collect();
    if files.is_empty() {
        return Ok(None);
    }
    for chunk in chunks(&files) {
        let mut args = vec![LITERAL_PATHS, "add", "--"];
        args.extend(chunk);
        run_with_index(repo, &scratch.untracked, &args)
            .map_err(|e| format!("git couldn't save the untracked files: {e}"))?;
    }
    let tree = run_with_index(repo, &scratch.untracked, &["write-tree"])?;
    commit_tree(repo, tree.trim(), &[], &format!("untracked files on {on}")).map(Some)
}

/// `files` in groups small enough for one command line each.
fn chunks<'a>(files: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut bytes = 0;
    for &file in files {
        let full = out
            .last()
            .is_none_or(|c| c.len() >= ADD_CHUNK || bytes + file.len() > ADD_CHUNK_BYTES);
        if full {
            out.push(Vec::new());
            bytes = 0;
        }
        bytes += file.len() + 1;
        if let Some(chunk) = out.last_mut() {
            chunk.push(file);
        }
    }
    out
}

/// A commit of `tree` with `parents`, never signed (`git stash` signs none either, and a
/// snapshot must not wait for a passphrase).
fn commit_tree(repo: &Path, tree: &str, parents: &[&str], message: &str) -> Result<String, String> {
    let mut args = vec!["commit-tree", "--no-gpg-sign", "-m", message];
    for p in parents {
        args.extend(["-p", p]);
    }
    args.push(tree);
    run(Some(repo), &args)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("git couldn't make the snapshot's commit: {e}"))
}

fn rev(repo: &Path, rev: &str) -> Result<String, String> {
    run(Some(repo), &["rev-parse", "--verify", "-q", rev])
        .map(|s| s.trim().to_owned())
        .map_err(|_| format!("{rev} is not a commit"))
}

/// The index files a snapshot works on, in the repository's git folder (where a split index
/// finds its shared part), removed when it's done: `copy`, the index as it is, and
/// `untracked`, a new one for the untracked files. Paths as git there reads them.
struct Scratch {
    copy: String,
    untracked: String,
    /// The same, as this app reaches them.
    files: [PathBuf; 2],
}

impl Scratch {
    fn new(repo: &Path) -> Result<Self, String> {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let git_dir = run(Some(repo), &["rev-parse", "--absolute-git-dir"])?;
        let git_dir = git_dir.trim_end_matches(['\n', '\r']).trim_end_matches('/');
        // A linked worktree's index is in its own git folder too.
        let index = format!("{git_dir}/index");
        let stem = format!(
            "{git_dir}/gitcraft-snapshot-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let copy = format!("{stem}.index");
        let untracked = format!("{stem}-untracked.index");
        let here = |p: &str| crate::remote::resolve(repo, p);
        let scratch = Scratch {
            files: [here(&copy), here(&untracked)],
            copy,
            untracked,
        };
        crate::remote::copy_keeping_time(&here(&index), &scratch.files[0])
            .map_err(|e| format!("Couldn't copy the index to work on: {e}"))?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = crate::remote::remove_file(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_every_kind_of_git() {
        assert_eq!(parse_version("git version 2.43.0\n"), Some((2, 43)));
        assert_eq!(parse_version("git version 2.35.1.windows.2"), Some((2, 35)));
        assert_eq!(
            parse_version("git version 2.39.3 (Apple Git-145)"),
            Some((2, 39))
        );
        assert_eq!(parse_version("git version 3.0"), Some((3, 0)));
        assert_eq!(parse_version("not git"), None);
        assert_eq!(parse_version("git version x"), None);
        assert!((2, 34) < STAGED_STASH_SINCE && (2, 35) >= STAGED_STASH_SINCE);
        assert!((3, 0) >= STAGED_STASH_SINCE);
    }

    #[test]
    fn stash_messages_read_like_gits_own() {
        let on = "main: 1234567 Fix: the thing";
        assert_eq!(stash_message(on, ""), "WIP on main: 1234567 Fix: the thing");
        assert_eq!(stash_message(on, "  half done "), "On main: half done");
        assert_eq!(
            stash_message("(no branch): 1234567 x", "ü"),
            "On (no branch): ü"
        );
    }

    #[test]
    fn untracked_files_are_added_in_short_command_lines() {
        assert!(chunks(&[]).is_empty());
        let many: Vec<String> = (0..1000).map(|i| format!("f{i}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let split = chunks(&many);
        assert_eq!(split.iter().map(Vec::len).sum::<usize>(), 1000);
        assert!(split.iter().all(|c| c.len() <= ADD_CHUNK));
        let long = "x".repeat(10_000);
        let split = chunks(&[&long, &long, &long]);
        assert_eq!(split.len(), 3);
        // A name longer than a whole chunk still goes, alone.
        let huge = "y".repeat(ADD_CHUNK_BYTES * 2);
        assert_eq!(chunks(&[&huge]), vec![vec![huge.as_str()]]);
    }
}
