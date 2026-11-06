//! What a [`Command`] means on disk: the repository it is about and what to open in it. Run
//! off the window's thread: it asks git (on another machine, for a WSL or SSH folder).

use std::path::{Component, Path, PathBuf};

use super::args::{Command, Range};
use crate::git;

/// A request made sense of: the repository's top folder and what to show there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub root: PathBuf,
    pub then: Then,
}

/// What to show once the repository is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// A file's history; its path in the repository, `/` between names.
    History(String),
    /// A file's blame, at a line (1 is the first).
    Blame {
        path: String,
        line: Option<u32>,
    },
    /// A commit, by its full hash.
    Commit(String),
    /// Two commits, by full hash, with the names they were given.
    Compare {
        base: String,
        base_label: String,
        target: String,
        target_label: String,
        since_fork: bool,
    },
}

/// Resolves `cmd`, its relative paths taken from `cwd`.
pub fn resolve(cmd: &Command, cwd: &Path) -> Result<Resolved, String> {
    match cmd {
        Command::Open(path) => Ok(Resolved {
            root: repo_root(&absolute(cwd, path))?,
            then: Then::Nothing,
        }),
        Command::Log(file) => {
            let (root, path) = file_in_repo(&absolute(cwd, file))?;
            Ok(Resolved {
                root,
                then: Then::History(path),
            })
        }
        Command::Blame { file, line } => {
            let (root, path) = file_in_repo(&absolute(cwd, file))?;
            Ok(Resolved {
                root,
                then: Then::Blame { path, line: *line },
            })
        }
        Command::Show(rev) => {
            let root = repo_root(cwd)?;
            let hash = git::resolve(&root, rev)?;
            Ok(Resolved {
                root,
                then: Then::Commit(hash),
            })
        }
        Command::Compare(Range {
            base,
            target,
            since_fork,
        }) => {
            let root = repo_root(cwd)?;
            let base_hash = git::resolve(&root, base)?;
            let target_hash = git::resolve(&root, target)?;
            Ok(Resolved {
                root,
                then: Then::Compare {
                    base: base_hash,
                    base_label: base.clone(),
                    target: target_hash,
                    target_label: target.clone(),
                    since_fork: *since_fork,
                },
            })
        }
    }
}

/// `path` as typed, made absolute against `cwd` (a folder of another machine is absolute).
pub(crate) fn absolute(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() || crate::remote::is_remote(path) {
        path.to_owned()
    } else {
        cwd.join(path)
    }
}

/// The top folder of the repository `path` is in, in the form tabs are told apart by.
pub(crate) fn repo_root(path: &Path) -> Result<PathBuf, String> {
    let root = git::toplevel(path)?;
    if crate::remote::is_remote(&root) {
        return Ok(root);
    }
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    Ok(crate::app::strip_verbatim(root))
}

/// The repository `file` is in, and its path there. A file that is gone (deleted, renamed) can
/// still have a history: only its folder has to be there.
fn file_in_repo(file: &Path) -> Result<(PathBuf, String), String> {
    let name = file
        .file_name()
        .ok_or_else(|| format!("{} is not a file", file.display()))?;
    let folder = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("{} is not a file", file.display()))?;
    if crate::remote::is_remote(file) {
        let root = repo_root(folder)?;
        let path = remote_relative(&root, file)
            .ok_or_else(|| format!("{} is not inside {}", file.display(), root.display()))?;
        return Ok((root, path));
    }
    if file.is_dir() {
        return Err(format!(
            "{} is a folder: give a file (or open the folder with gitcraft {0})",
            file.display()
        ));
    }
    if !folder.is_dir() {
        return Err(format!("No such folder: {}", folder.display()));
    }
    let root = repo_root(folder)?;
    let folder = crate::app::strip_verbatim(
        std::fs::canonicalize(folder).map_err(|e| format!("{}: {e}", folder.display()))?,
    );
    let path = relative(&root, &folder.join(name))
        .ok_or_else(|| format!("{} is not inside {}", file.display(), root.display()))?;
    Ok((root, path))
}

/// `file`'s path inside `root`, `/` between names; `None` when it isn't inside.
pub(crate) fn relative(root: &Path, file: &Path) -> Option<String> {
    let rest = file.strip_prefix(root).ok()?;
    let mut names = Vec::new();
    for part in rest.components() {
        match part {
            Component::Normal(name) => names.push(name.to_string_lossy().into_owned()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!names.is_empty()).then(|| names.join("/"))
}

/// The same for a file of another machine, by its path there.
fn remote_relative(root: &Path, file: &Path) -> Option<String> {
    let (root, file) = (crate::remote::parse(root)?, crate::remote::parse(file)?);
    if root.host != file.host {
        return None;
    }
    let rest = file.path.strip_prefix(&root.path)?.strip_prefix('/')?;
    let fine = !rest.is_empty() && rest.split('/').all(|n| !n.is_empty() && n != "..");
    fine.then(|| rest.replace("/./", "/"))
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
