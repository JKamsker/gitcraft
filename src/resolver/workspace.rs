//! Where a resolving agent works: a scratch worktree of the repository (out of the user's way,
//! not listed among the worktrees, removed when done; see `git::Scratch`), with the operation
//! that conflicted replayed in it so that the agent sees the whole of it, and each conflicted
//! file as it is in the user's checkout (what the user already chose there included).
//!
//! The user's checkout is not touched until the agent's work is applied, file by file, and only
//! where the file is still as it was when the agent started.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    /// The agent left a link (or anything but a file) in its place: kept as such.
    agent_not_plain: bool,
}

/// The two sides of the operation, as the agent is told them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
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

/// The agent's scratch worktree and what it was given; or, once it is done, its work kept
/// (see [`Workspace::keep`]).
pub struct Workspace {
    /// Where the agent's version of each file is.
    dir: PathBuf,
    scratch: Option<Scratch>,
    /// The folder its work is kept in, when it is.
    kept: Option<PathBuf>,
    pub files: Vec<Conflicted>,
    pub sides: Sides,
}

/// A file of a kept workspace, as `workspace.json` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct KeptFile {
    path: String,
    conflicts: usize,
    /// It was in the checkout (under `original/`).
    original: bool,
    /// "file" (under `resolved/`), "deleted" or "not_plain".
    agent: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct KeptWorkspace {
    files: Vec<KeptFile>,
    sides: Sides,
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
    /// It had no markers to resolve (one side deleted or renamed it, or it is binary), and the
    /// agent neither changed it nor said it was done with it: the user decides.
    Untouched,
    /// The user changed it in their checkout since: it isn't applied over their work.
    ChangedMeanwhile,
    /// It, or a folder on its way, isn't a plain file or folder (a link): never applied.
    NotPlain,
}

/// A file as the agent was given it and as it left it (`None`: not there).
pub type BeforeAndAfter<'a> = (Option<&'a [u8]>, Option<Vec<u8>>);

/// What applying wrote, and what it couldn't.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub written: Vec<String>,
    pub failed: Vec<(String, String)>,
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

/// Whether `path` of the checkout at `top` is reached without a link: none of its folders, nor
/// it, is one (an agent could link a file of the checkout to a secret elsewhere, to have it
/// applied and committed). `Ok(false)` when it, or a folder on its way, isn't there.
fn plain(top: &Path, path: &str) -> Result<bool, String> {
    let mut at = top.to_path_buf();
    for part in path.split(['/', '\\']) {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("{path} goes through a link"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("couldn't look at {path}: {e}")),
        }
    }
    Ok(true)
}

/// Reads `path` of the checkout at `top`: `None` when there is no such file. One reached
/// through a link, or that isn't a file, is an error.
fn read(top: &Path, path: &str) -> Result<Option<Vec<u8>>, String> {
    if !plain(top, path)? {
        return Ok(None);
    }
    let file = top.join(path);
    if !file.is_file() {
        return Err(format!("{path} isn't a file"));
    }
    std::fs::read(&file)
        .map(Some)
        .map_err(|e| format!("couldn't read {path}: {e}"))
}

/// Writes `bytes` to `path` of the checkout at `top` (removes it for `None`), never through a
/// link.
fn write(top: &Path, path: &str, bytes: Option<&[u8]>) -> Result<(), String> {
    plain(top, path)?;
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
                agent_not_plain: false,
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
            dir: scratch.path.clone(),
            scratch: Some(scratch),
            kept: None,
            files: given,
            sides,
        })
    }

    /// The folder the agent works in (or its work is kept in).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How each file stands: the agent's version, against the checkout at `repo` now when one
    /// is given (`None`: while the operation is put aside, the checkout has none of it).
    /// `done`: the files (or steps named after them) the agent reported done.
    pub fn check(&self, repo: Option<&Path>, done: &[String]) -> Vec<(String, FileResult)> {
        self.files
            .iter()
            .map(|file| (file.path.clone(), self.check_one(repo, file, done)))
            .collect()
    }

    fn check_one(&self, repo: Option<&Path>, file: &Conflicted, done: &[String]) -> FileResult {
        if let Some(repo) = repo {
            let Ok(now) = read(repo, &file.path) else {
                return FileResult::NotPlain;
            };
            if now != file.original {
                return FileResult::ChangedMeanwhile;
            }
        }
        if file.agent_not_plain {
            return FileResult::NotPlain;
        }
        let Ok(theirs) = read(self.dir(), &file.path) else {
            return FileResult::NotPlain;
        };
        // A file with markers is resolved once they are gone; one without is only when the
        // agent changed it or said it is done with it.
        let decided = theirs != file.original || done.contains(&file.path);
        match theirs {
            Some(text) if file.conflicts > 0 && has_markers(&text) => FileResult::StillConflicted,
            _ if file.conflicts == 0 && !decided => FileResult::Untouched,
            Some(_) => FileResult::Resolved,
            None => FileResult::Deleted,
        }
    }

    /// Writes the agent's version of each of `paths` that is ready into the checkout at `repo`,
    /// each on its own: one that fails doesn't stop the others. One the user changed
    /// meanwhile, or that isn't resolved, is left as it is.
    pub fn apply(&self, repo: &Path, paths: &[String], done: &[String]) -> Applied {
        let mut applied = Applied::default();
        for (path, result) in self.check(Some(repo), done) {
            if !result.applies() || !paths.contains(&path) {
                continue;
            }
            let copied =
                read(self.dir(), &path).and_then(|theirs| write(repo, &path, theirs.as_deref()));
            match copied {
                Ok(()) => applied.written.push(path),
                Err(e) => applied.failed.push((path, e)),
            }
        }
        applied
    }

    /// `path` as the agent was given it, and as it left it, for showing what it did: `None`
    /// for one that isn't there; an error for one that isn't a plain file.
    pub fn before_and_after(&self, path: &str) -> Result<BeforeAndAfter<'_>, String> {
        let file = self
            .files
            .iter()
            .find(|f| f.path == path)
            .ok_or_else(|| format!("{path} wasn't given to the agent"))?;
        Ok((file.original.as_deref(), read(self.dir(), path)?))
    }

    /// Keeps the agent's work in the folder `to` (of the repository's git folder: it outlasts
    /// gitcraft, unlike the scratch worktree): each file as it was given (`original/`) and as
    /// the agent left it (`resolved/`), and what is known of them (`workspace.json`). The
    /// workspace of what is kept.
    pub fn keep(&self, to: &Path) -> Result<Workspace, String> {
        let (originals, resolved) = (to.join("original"), to.join("resolved"));
        std::fs::create_dir_all(&resolved)
            .map_err(|e| format!("couldn't make {}: {e}", resolved.display()))?;
        let mut listed = Vec::new();
        let mut files = Vec::new();
        for file in &self.files {
            if let Some(bytes) = &file.original {
                write(&originals, &file.path, Some(bytes))?;
            }
            let agent = match read(self.dir(), &file.path) {
                _ if file.agent_not_plain => "not_plain",
                Ok(Some(bytes)) => {
                    write(&resolved, &file.path, Some(&bytes))?;
                    "file"
                }
                Ok(None) => "deleted",
                Err(_) => "not_plain",
            };
            listed.push(KeptFile {
                path: file.path.clone(),
                conflicts: file.conflicts,
                original: file.original.is_some(),
                agent: agent.to_owned(),
            });
            files.push(Conflicted {
                agent_not_plain: agent == "not_plain",
                ..file.clone()
            });
        }
        let kept = KeptWorkspace {
            files: listed,
            sides: self.sides.clone(),
        };
        let text = serde_json::to_string_pretty(&kept).map_err(|e| e.to_string())?;
        std::fs::write(to.join("workspace.json"), text)
            .map_err(|e| format!("couldn't keep the agent's work: {e}"))?;
        Ok(Workspace {
            dir: resolved,
            scratch: None,
            kept: Some(to.to_path_buf()),
            files,
            sides: self.sides.clone(),
        })
    }

    /// The agent's work kept in `from` by [`Workspace::keep`].
    pub fn load(from: &Path) -> Result<Workspace, String> {
        let text = std::fs::read_to_string(from.join("workspace.json"))
            .map_err(|e| format!("the agent's work isn't there: {e}"))?;
        let kept: KeptWorkspace = serde_json::from_str(&text)
            .map_err(|e| format!("the agent's work can't be read: {e}"))?;
        let mut files = Vec::with_capacity(kept.files.len());
        for f in kept.files {
            let path = checked(&f.path)?.to_owned();
            let original = if f.original {
                read(&from.join("original"), &path)?
            } else {
                None
            };
            files.push(Conflicted {
                path,
                original,
                conflicts: f.conflicts,
                agent_not_plain: f.agent == "not_plain",
            });
        }
        Ok(Workspace {
            dir: from.join("resolved"),
            scratch: None,
            kept: Some(from.to_path_buf()),
            files,
            sides: kept.sides,
        })
    }

    /// Removes the scratch worktree (or the work kept) now, saying why if it couldn't be.
    pub fn remove(self) -> Result<(), String> {
        match (self.scratch, self.kept) {
            (Some(scratch), _) => scratch.remove(),
            (None, Some(kept)) => match std::fs::remove_dir_all(&kept) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    Err(format!("couldn't remove {}: {e}", kept.display()))
                }
                _ => Ok(()),
            },
            (None, None) => Ok(()),
        }
    }

    /// Removes it, trying again for a while: on Windows a program that just ended may still
    /// hold its folder for a moment.
    pub fn remove_patiently(mut self) -> Result<(), String> {
        match self.scratch.take() {
            Some(scratch) => scratch.remove_retrying(5, std::time::Duration::from_millis(400)),
            None => self.remove(),
        }
    }
}

#[cfg(test)]
mod tests;
