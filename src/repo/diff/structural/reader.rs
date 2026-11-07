//! Read exactly the two Git sides, distinguishing absent files from read failures.

use std::path::Path;

use crate::git::{self, Area, ChangedFile};
use crate::repo::DiffKey;

use super::model::{self, Structural};
use super::tool;

pub(super) const MAX_BYTES: usize = 2 * 1024 * 1024;
type At = Option<(Option<String>, String)>;

pub(super) fn load(repo: &Path, key: &DiffKey) -> Result<Structural, String> {
    let (old, new) = read_sides(repo, key)?;
    let json = tool::compare(repo, key.path(), &old, &new)?;
    model::parse(&json, &old, &new)
}

pub(super) fn read_sides(repo: &Path, key: &DiffKey) -> Result<(String, String), String> {
    let (old, new) = locations(repo, key)?;
    Ok((read(repo, old)?, read(repo, new)?))
}

fn locations(repo: &Path, key: &DiffKey) -> Result<(At, At), String> {
    let at = |rev: Option<&str>, path: &str| Some((rev.map(str::to_owned), path.to_owned()));
    let changed = |files: Vec<ChangedFile>, path: &str, old: Option<&str>, new: Option<&str>| {
        let file = files.into_iter().find(|f| f.path == path).ok_or_else(|| {
            "This file is no longer in the comparison. Refresh its diff.".to_owned()
        })?;
        let before = (file.status != 'A' && old.is_some())
            .then(|| at(old, file.old_path.as_deref().unwrap_or(path)))
            .flatten();
        let after = (file.status != 'D').then(|| at(new, path)).flatten();
        Ok((before, after))
    };
    match key {
        DiffKey::Commit { hash, path } => {
            let detail = git::commit_detail(repo, hash)?;
            changed(
                detail.files,
                path,
                detail.parents.first().map(String::as_str),
                Some(hash),
            )
        }
        DiffKey::Range { from, to, path } => changed(
            git::diff_files(repo, from, to.as_deref())?,
            path,
            Some(from),
            to.as_deref(),
        ),
        DiffKey::Work(file) => match file.area {
            Area::Staged => Ok((
                (file.code != 'A')
                    .then(|| at(Some("HEAD"), file.old_path.as_deref().unwrap_or(&file.path)))
                    .flatten(),
                (file.code != 'D')
                    .then(|| at(Some(""), &file.path))
                    .flatten(),
            )),
            Area::Unstaged => Ok((
                at(Some(""), &file.path),
                (file.code != 'D').then(|| at(None, &file.path)).flatten(),
            )),
            Area::Untracked => Ok((None, at(None, &file.path))),
            Area::Conflicted => Err("Resolve the conflict before comparing its syntax.".to_owned()),
        },
    }
}

fn read(repo: &Path, side: At) -> Result<String, String> {
    let Some((rev, path)) = side else {
        return Ok(String::new());
    };
    let bytes = match rev {
        Some(rev) => {
            let object = if rev.is_empty() { format!(":0:{path}") } else { format!("{rev}:{path}") };
            // Resolve once so an index/HEAD move between the size check and read cannot
            // turn a small blob into an unbounded one.
            let blob = git::run(Some(repo), &["rev-parse", "--verify", "--end-of-options", &object])?;
            let blob = blob.trim();
            let size = git::run(Some(repo), &["cat-file", "-s", "--", blob])?;
            if size.trim().parse::<usize>().map_err(|_| "Couldn't read this blob's size")? > MAX_BYTES {
                return Err("Syntax comparison is limited to 2 MiB per file.".to_owned());
            }
            git::run_bytes(Some(repo), &["cat-file", "blob", blob])?
        }
        None => match crate::remote::read_within(repo, &path, MAX_BYTES as u64) {
            Some(result) => result.map_err(|e| e.to_string())?,
            None => crate::safe_read::read_inside(repo, &path, MAX_BYTES as u64),
        }.ok_or("Couldn't read a regular file within 2 MiB from the working tree (links are not followed).")?,
    };
    if bytes.len() > MAX_BYTES {
        return Err("Syntax comparison is limited to 2 MiB per file.".to_owned());
    }
    if bytes.contains(&0) {
        return Err("Syntax comparison is available for text files only.".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "Syntax comparison needs UTF-8 text.".to_owned())
}
