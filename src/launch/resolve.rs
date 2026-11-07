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
    /// A linked file's blame: at the linked commit (its full hash) when the clone has it, else
    /// as the clone has the file.
    FileAt {
        rev: Option<String>,
        path: String,
        line: Option<u32>,
    },
    /// A commit, by its full hash.
    Commit(String),
    /// A pull request, by its number.
    PullRequest(u64),
    /// No clone of a linked repository was found: offer to clone it from this URL.
    Clone(String),
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
    resolve_among(cmd, cwd, &[])
}

/// [`resolve`], a link's clone looked for among `clones` (the open tabs, the recent
/// repositories).
pub fn resolve_among(
    cmd: &Command,
    cwd: &Path,
    clones: &[super::open_link::Known],
) -> Result<Resolved, String> {
    match cmd {
        Command::Clone | Command::PickFolder => {
            Err("This command opens a dialog in the window".to_owned())
        }
        Command::Link(link) => super::open_link::resolve_link(link, clones),
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

/// The repository `file` is in, and its path there as git names it: git says where the
/// file's folder is in the repository (`rev-parse --show-prefix`, which follows links, `~` and
/// the case the folder really has), the same here and on another machine. A file that is gone
/// (deleted, renamed) can still have a history, also when its folder is gone too.
fn file_in_repo(file: &Path) -> Result<(PathBuf, String), String> {
    let not_a_file = || format!("{} is not a file", file.display());
    let name = match file.components().next_back() {
        Some(Component::Normal(name)) => name.to_string_lossy().into_owned(),
        _ => return Err(not_a_file()),
    };
    let folder = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(not_a_file)?;
    let remote = crate::remote::is_remote(file);
    if !remote && file.is_dir() {
        return Err(format!(
            "{} is a folder: give a file (or open the folder with gitcraft {0})",
            file.display()
        ));
    }
    let (existing, missing) = if remote {
        (folder.to_owned(), Vec::new())
    } else {
        nearest_folder(folder)?
    };
    // In the case the folder really has (Windows and macOS take any): git names it as given.
    let existing = if remote {
        existing
    } else {
        std::fs::canonicalize(&existing)
            .map(crate::app::strip_verbatim)
            .unwrap_or(existing)
    };
    let root = repo_root(&existing)?;
    let prefix = git::run(Some(&existing), &["rev-parse", "--show-prefix"])?;
    let prefix = prefix.trim_end_matches(['\n', '\r']);
    // The name as it is on disk (the one typed may differ in case on Windows and macOS).
    let name = (!remote)
        .then(|| std::fs::canonicalize(file).ok())
        .flatten()
        .and_then(|real| real.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or(name);
    let mut path = prefix.to_owned();
    for part in missing.iter().rev() {
        path.push_str(part);
        path.push('/');
    }
    path.push_str(&name);
    Ok((root, path))
}

/// The nearest folder of `folder` (itself, or one it is in) that is there, and the names of
/// those under it that aren't, innermost first.
fn nearest_folder(folder: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let mut dir = folder;
    let mut missing = Vec::new();
    while !dir.is_dir() {
        match (dir.file_name(), dir.parent()) {
            (Some(name), Some(parent)) if !parent.as_os_str().is_empty() => {
                missing.push(name.to_string_lossy().into_owned());
                dir = parent;
            }
            _ => return Err(format!("No such folder: {}", folder.display())),
        }
    }
    Ok((dir.to_owned(), missing))
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
