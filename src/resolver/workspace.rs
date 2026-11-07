//! Where a resolving agent works: a scratch worktree of the repository (out of the user's way,
//! not listed among the worktrees, removed when done; see `git::Scratch`), with the operation
//! that conflicted replayed in it so that the agent sees the whole of it, and each conflicted
//! file as it is in the user's checkout (what the user already chose there included).
//!
//! The user's checkout is not touched until the agent's work is applied, file by file, and only
//! where the file is still as it was when the agent started.

use std::path::Path;

use crate::git::{self, Scratch};

/// The name the scratch worktrees made for this have (see `git::Scratch::add_empty`).
pub const SCRATCH_LABEL: &str = "resolve";

/// A conflicted file the agent is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflicted {
    /// Relative to the top of the repository, as git status names it.
    pub path: String,
    /// The file in the user's checkout when the agent started; `None` when there was none (a
    /// side deleted it).
    original: Option<Vec<u8>>,
    /// How many conflicts it had then.
    pub conflicts: usize,
}

/// The two sides of the operation, as the agent is told them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sides {
    /// What it is: "merge", "cherry-pick", "revert", "rebase" (empty when gitcraft can't tell).
    pub operation: String,
    /// The commit checked out, and its branch if one.
    pub ours: String,
    pub ours_name: String,
    /// The commit being brought in, and what names it (a branch, a tag).
    pub theirs: Option<String>,
    pub theirs_name: Option<String>,
}

/// The agent's scratch worktree and what it was given.
pub struct Workspace {
    scratch: Scratch,
    pub files: Vec<Conflicted>,
    pub sides: Sides,
}

/// How a file stands after the agent's work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileResult {
    /// No conflict marker left: ready to apply.
    Resolved,
    /// The agent removed it (the side that deleted it won).
    Deleted,
    /// Conflict markers are still in it.
    StillConflicted,
    /// The user changed it in their checkout since: it isn't applied over their work.
    ChangedMeanwhile,
}

impl FileResult {
    pub fn applies(&self) -> bool {
        matches!(self, FileResult::Resolved | FileResult::Deleted)
    }
}

/// Whether `text` has a conflict marker at the start of a line.
pub fn has_markers(text: &[u8]) -> bool {
    text.split(|&b| b == b'\n').any(is_marker)
}

/// How many conflicts `text` has.
pub fn count_conflicts(text: &[u8]) -> usize {
    text.split(|&b| b == b'\n')
        .filter(|line| line.starts_with(b"<<<<<<<"))
        .count()
}

fn is_marker(line: &[u8]) -> bool {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let marker = |m: &[u8]| line == m || (line.starts_with(m) && line.get(7) == Some(&b' '));
    marker(b"<<<<<<<") || marker(b">>>>>>>")
}

/// The operation `repo` is in the middle of, its commit and how to replay it in a worktree of
/// its own: `merge`, `cherry-pick`, `revert`, or a rebase's commit (picked again).
fn in_progress(repo: &Path) -> Option<(&'static str, String, Vec<&'static str>)> {
    let head = |name: &str| {
        git::run(Some(repo), &["rev-parse", "-q", "--verify", name])
            .ok()
            .and_then(|out| out.lines().next().map(|l| l.trim().to_owned()))
            .filter(|h| !h.is_empty())
    };
    if let Some(h) = head("MERGE_HEAD") {
        return Some(("merge", h, vec!["merge", "--no-commit", "--no-ff", "-q"]));
    }
    if let Some(h) = head("CHERRY_PICK_HEAD") {
        return Some(("cherry-pick", h, vec!["cherry-pick", "--no-commit"]));
    }
    if let Some(h) = head("REVERT_HEAD") {
        return Some(("revert", h, vec!["revert", "--no-commit"]));
    }
    if let Some(h) = head("REBASE_HEAD") {
        return Some(("rebase", h, vec!["cherry-pick", "--no-commit"]));
    }
    None
}

/// The branch, tag or other name of `commit`, if git knows one.
fn name_of(repo: &Path, commit: &str) -> Option<String> {
    git::run(
        Some(repo),
        &["name-rev", "--name-only", "--no-undefined", commit],
    )
    .ok()
    .map(|n| n.trim().to_owned())
    .filter(|n| !n.is_empty())
}

fn sides(repo: &Path) -> (Sides, Option<Vec<String>>) {
    let ours = git::run(Some(repo), &["rev-parse", "HEAD"])
        .map(|h| h.trim().to_owned())
        .unwrap_or_default();
    let ours_name = git::run(Some(repo), &["symbolic-ref", "-q", "--short", "HEAD"])
        .map(|b| b.trim().to_owned())
        .unwrap_or_else(|_| "detached HEAD".to_owned());
    let Some((operation, theirs, replay)) = in_progress(repo) else {
        let sides = Sides {
            ours,
            ours_name,
            ..Sides::default()
        };
        return (sides, None);
    };
    let mut replay: Vec<String> = replay.iter().map(|w| (*w).to_owned()).collect();
    replay.push(theirs.clone());
    let sides = Sides {
        operation: operation.to_owned(),
        theirs_name: name_of(repo, &theirs),
        theirs: Some(theirs),
        ours,
        ours_name,
    };
    (sides, Some(replay))
}

/// Reads `path` of the checkout at `top`: `None` when there is no such file.
fn read(top: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(top.join(path)) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("couldn't read {path}: {e}")),
    }
}

/// Writes `bytes` to `path` of the checkout at `top` (removes it for `None`).
fn write(top: &Path, path: &str, bytes: Option<&[u8]>) -> Result<(), String> {
    let file = top.join(path);
    match bytes {
        Some(bytes) => {
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("couldn't make the folder of {path}: {e}"))?;
            }
            std::fs::write(&file, bytes).map_err(|e| format!("couldn't write {path}: {e}"))
        }
        None => match std::fs::remove_file(&file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {path}: {e}")),
        },
    }
}

/// A path git status gave, checked to stay inside the checkout.
fn checked(path: &str) -> Result<&str, String> {
    crate::mcp::args::repo_path(path)
}

impl Workspace {
    /// Makes the scratch worktree for resolving `files` of the checkout at `repo` (on this
    /// machine), replays the operation in progress there, and puts each file in it as it is
    /// in the checkout.
    pub fn prepare(repo: &Path, files: &[String]) -> Result<Workspace, String> {
        if crate::remote::is_remote(repo) {
            return Err(
                "Resolving in the background works for repositories on this machine \
                        only, for now"
                    .to_owned(),
            );
        }
        if files.is_empty() {
            return Err("There is no conflicted file to resolve".to_owned());
        }
        let mut given = Vec::with_capacity(files.len());
        for path in files {
            let path = checked(path)?;
            let original = read(repo, path)?;
            let conflicts = original.as_deref().map_or(0, count_conflicts);
            given.push(Conflicted {
                path: path.to_owned(),
                original,
                conflicts,
            });
        }
        let (sides, replay) = sides(repo);
        let scratch = Scratch::add_empty(repo, SCRATCH_LABEL, "HEAD")?;
        scratch.checkout("HEAD")?;
        if let Some(replay) = replay {
            // It conflicts there too (that is why it is replayed): what matters is that the
            // rest of it is in place. gitcraft's own scratch worktree has no hooks to run.
            let mut args = vec![
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "rerere.enabled=false",
            ];
            args.extend(replay.iter().map(String::as_str));
            let _ = git::run(Some(&scratch.path), &args);
        }
        for file in &given {
            write(&scratch.path, &file.path, file.original.as_deref())?;
        }
        Ok(Workspace {
            scratch,
            files: given,
            sides,
        })
    }

    /// The folder the agent works in.
    pub fn dir(&self) -> &Path {
        &self.scratch.path
    }

    /// How each file stands: the agent's version against the checkout at `repo` now.
    pub fn check(&self, repo: &Path) -> Vec<(String, FileResult)> {
        self.files
            .iter()
            .map(|file| {
                let result = match (read(repo, &file.path), read(self.dir(), &file.path)) {
                    (Ok(now), _) if now != file.original => FileResult::ChangedMeanwhile,
                    (Err(_), _) => FileResult::ChangedMeanwhile,
                    (Ok(_), Ok(None)) => FileResult::Deleted,
                    (Ok(_), Ok(Some(text))) if !has_markers(&text) => FileResult::Resolved,
                    (Ok(_), _) => FileResult::StillConflicted,
                };
                (file.path.clone(), result)
            })
            .collect()
    }

    /// Writes the agent's version of each of `paths` that is ready into the checkout at `repo`:
    /// the paths written, for marking them resolved. One the user changed meanwhile, or that
    /// still has conflicts, is left as it is.
    pub fn apply(&self, repo: &Path, paths: &[String]) -> Result<Vec<String>, String> {
        let ready: Vec<String> = self
            .check(repo)
            .into_iter()
            .filter(|(path, result)| result.applies() && paths.contains(path))
            .map(|(path, _)| path)
            .collect();
        for path in &ready {
            let theirs = read(self.dir(), path)?;
            write(repo, path, theirs.as_deref())?;
        }
        Ok(ready)
    }

    /// Removes the scratch worktree now, saying why if it couldn't be.
    pub fn remove(self) -> Result<(), String> {
        self.scratch.remove()
    }
}

#[cfg(test)]
mod tests;
