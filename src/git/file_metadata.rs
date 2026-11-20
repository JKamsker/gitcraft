//! File metadata edits that otherwise take a terminal: the executable bit of a file in the
//! index, and a rename that changes only the letter case of a tracked file's name.
//!
//! Both are planned from a [`Snapshot`] read in the background, then run as plain Git
//! commands with exact inverse commands for undo. Neither touches the file's contents: the
//! executable bit is set on the index entry as it is (`update-index --cacheinfo`, not
//! `--chmod`, which would also stage the working-tree contents), and a case-only rename
//! moves the file through a temporary name, so it works the same on case-insensitive
//! filesystems (Windows, macOS) and keeps staged and unstaged edits with the file.

use std::collections::BTreeSet;
use std::path::Path;

use super::{LITERAL_PATHS, run};

/// A regular file's index mode, not executable / executable.
const PLAIN: &str = "100644";
const EXECUTABLE: &str = "100755";
/// Tried after the base temporary name before giving up on finding a free one.
const TEMP_TRIES: usize = 100;

/// What is known of one tracked file when a metadata edit is planned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// Repository-relative path, `/`-separated.
    pub(crate) path: String,
    /// `(mode, blob)` of its stage-0 index entry; `None` when it isn't in the index.
    pub(crate) entry: Option<(String, String)>,
    /// It has conflict stages in the index.
    pub(crate) conflicted: bool,
    /// The working-tree file is gone (an unstaged deletion).
    pub(crate) missing: bool,
    /// The names in its folder (first path component below it), tracked or not.
    pub(crate) siblings: BTreeSet<String>,
}

/// Commands that make an edit, and the commands that take it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The operation's label (an untranslated identity, like other operation labels).
    pub(crate) label: String,
    pub(crate) ops: Vec<Vec<String>>,
    pub(crate) undo: Vec<Vec<String>>,
    /// Shown before and after: modes for the executable bit, paths for a rename.
    pub(crate) from: String,
    pub(crate) to: String,
}

/// Reads what planning an edit of `path` needs. `siblings`: also list its folder (for a
/// rename; the executable bit doesn't need it).
pub(crate) fn snapshot(repo: &Path, path: &str, siblings: bool) -> Result<Snapshot, String> {
    let staged = run(
        Some(repo),
        &[LITERAL_PATHS, "ls-files", "-s", "-z", "--", path],
    )?;
    let mut entry = None;
    let mut conflicted = false;
    for line in staged.split('\0') {
        let Some((meta, name)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split(' ');
        let (Some(mode), Some(hash), Some(stage)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if name != path {
            continue;
        }
        if stage == "0" {
            entry = Some((mode.to_owned(), hash.to_owned()));
        } else {
            conflicted = true;
        }
    }
    let deleted = run(
        Some(repo),
        &[LITERAL_PATHS, "ls-files", "-d", "-z", "--", path],
    )?;
    let missing = deleted.split('\0').any(|p| p == path);
    let siblings = if siblings {
        folder_names(repo, path)?
    } else {
        BTreeSet::new()
    };
    Ok(Snapshot {
        path: path.to_owned(),
        entry,
        conflicted,
        missing,
        siblings,
    })
}

/// The names directly in `path`'s folder: tracked files and everything else there, ignored
/// ones too (`ls-files -o` without excludes); an untracked folder shows as one name.
fn folder_names(repo: &Path, path: &str) -> Result<BTreeSet<String>, String> {
    let (folder, _) = split(path);
    let mut args = vec![LITERAL_PATHS, "ls-files", "-z", "-c", "-o", "--directory"];
    if !folder.is_empty() {
        args.extend(["--", folder]);
    }
    let listed = run(Some(repo), &args)?;
    Ok(listed
        .split('\0')
        .filter_map(|p| p.strip_prefix(folder))
        .filter_map(|rest| rest.split('/').next())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

/// `dir/name` → (`dir/`, `name`); a file at the top has no folder (`""`).
fn split(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(at) => path.split_at(at + 1),
        None => ("", path),
    }
}

/// The stage-0 entry to edit, or why the file can't be edited.
fn editable(snapshot: &Snapshot) -> Result<(&str, &str), String> {
    let refuse = |key: &'static str| Err(crate::i18n::text(key).to_owned());
    if snapshot.conflicted {
        return refuse("file-metadata-refused-conflicted");
    }
    match &snapshot.entry {
        Some((mode, hash)) => Ok((mode.as_str(), hash.as_str())),
        None => refuse("file-metadata-refused-untracked"),
    }
}

/// Sets the executable bit of the index entry if it is clear, clears it if it is set. Only
/// the entry's mode changes: its blob stays, so unstaged edits stay unstaged.
pub(crate) fn executable_plan(snapshot: &Snapshot) -> Result<Plan, String> {
    let (mode, hash) = editable(snapshot)?;
    let (to, verb) = match mode {
        PLAIN => (EXECUTABLE, "Make"),
        EXECUTABLE => (PLAIN, "Clear the executable bit of"),
        _ => {
            return Err(crate::i18n::message(
                "file-metadata-refused-not-regular",
                &[("mode", mode.to_owned().into())],
            ));
        }
    };
    let set = |mode: &str| {
        let args = ["update-index", "--cacheinfo", mode, hash, &snapshot.path];
        args.map(str::to_owned).to_vec()
    };
    let label = match to {
        EXECUTABLE => format!("{verb} {} executable", snapshot.path),
        _ => format!("{verb} {}", snapshot.path),
    };
    Ok(Plan {
        label,
        ops: vec![set(to)],
        undo: vec![set(mode)],
        from: mode.to_owned(),
        to: to.to_owned(),
    })
}

/// Whether `mode` is the executable regular-file mode.
pub(crate) fn is_executable(mode: &str) -> bool {
    mode == EXECUTABLE
}

/// Renames the file to `name` (in the same folder), which differs from its name only in
/// letter case, through a temporary name nothing in the folder has.
pub(crate) fn case_rename_plan(snapshot: &Snapshot, name: &str) -> Result<Plan, String> {
    editable(snapshot)?;
    let (folder, old) = split(&snapshot.path);
    let refuse = |key: &'static str| Err(crate::i18n::text(key).to_owned());
    if name.is_empty() || name.contains(['/', '\\']) {
        return refuse("file-metadata-refused-bad-name");
    }
    if name == old {
        return refuse("file-metadata-refused-same-name");
    }
    if fold(name) != fold(old) {
        return refuse("file-metadata-refused-not-case-only");
    }
    if snapshot.missing {
        return refuse("file-metadata-refused-missing");
    }
    if let Some(other) = snapshot
        .siblings
        .iter()
        .find(|s| s.as_str() != old && fold(s) == fold(name))
    {
        return Err(crate::i18n::message(
            "file-metadata-refused-collision",
            &[("name", format!("{folder}{other}").into())],
        ));
    }
    let temp = temporary_name(old, &snapshot.siblings)
        .ok_or_else(|| crate::i18n::text("file-metadata-refused-no-temp").to_owned())?;
    let (from, to, via) = (
        snapshot.path.clone(),
        format!("{folder}{name}"),
        format!("{folder}{temp}"),
    );
    let mv = |a: &str, b: &str| ["mv", "--", a, b].map(str::to_owned).to_vec();
    Ok(Plan {
        label: format!("Rename {from} to {to}"),
        ops: vec![mv(&from, &via), mv(&via, &to)],
        undo: vec![mv(&to, &via), mv(&via, &from)],
        from,
        to,
    })
}

/// A name for the step between: not in the folder in any letter case.
fn temporary_name(old: &str, siblings: &BTreeSet<String>) -> Option<String> {
    let taken: BTreeSet<String> = siblings.iter().map(|s| s.to_lowercase()).collect();
    let base = format!(".{old}.gitcraft-case-rename");
    std::iter::once(base.clone())
        .chain((1..TEMP_TRIES).map(|n| format!("{base}-{n}")))
        .find(|candidate| !taken.contains(&fold(candidate)))
}

/// A name as a case-insensitive filesystem compares it.
fn fold(name: &str) -> String {
    name.to_lowercase()
}

#[cfg(test)]
mod tests;
