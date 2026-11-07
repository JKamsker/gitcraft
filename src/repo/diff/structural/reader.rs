//! Read exactly the two Git sides, distinguishing absent files from read failures.

use std::path::Path;
use std::time::{Duration, Instant};

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
    if crate::remote::is_remote(repo) {
        return Err(
            "Syntax comparison is currently available for local repositories. Showing the patch."
                .to_owned(),
        );
    }
    let read = Read {
        repo,
        deadline: Instant::now() + Duration::from_secs(20),
    };
    let (old, new) = locations(&read, key)?;
    Ok((read_side(&read, old)?, read_side(&read, new)?))
}

struct Read<'a> {
    repo: &'a Path,
    deadline: Instant,
}
impl Read<'_> {
    fn bytes(&self, args: &[&str], limit: usize) -> Result<Vec<u8>, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Syntax preparation timed out. Showing the patch.".to_owned());
        }
        git::syntax_read(self.repo, args, limit, remaining)
    }
    fn text(&self, args: &[&str]) -> Result<String, String> {
        String::from_utf8(self.bytes(args, MAX_BYTES)?)
            .map_err(|_| "Syntax metadata is not UTF-8".to_owned())
    }
    fn files(&self, from: Option<&str>, to: Option<&str>) -> Result<Vec<ChangedFile>, String> {
        let mut args = match from {
            Some(from) => vec![
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "-M",
                "--name-status",
                "-z",
                "--end-of-options",
                from,
            ],
            None => vec![
                "diff-tree",
                "--root",
                "--no-commit-id",
                "-r",
                "--no-ext-diff",
                "--no-textconv",
                "--name-status",
                "-z",
                "--end-of-options",
            ],
        };
        args.extend(to);
        args.push("--");
        Ok(git::parse_name_status(&self.text(&args)?))
    }
}

fn locations(read: &Read<'_>, key: &DiffKey) -> Result<(At, At), String> {
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
        DiffKey::Remerge { .. } => Err(
            "Syntax comparison is not available for merge-resolution diffs. Showing the patch."
                .to_owned(),
        ),
        DiffKey::Commit { hash, path } => {
            let parents = read.text(&[
                "show",
                "--no-show-signature",
                "--no-notes",
                "-s",
                "--format=%P",
                "--end-of-options",
                hash,
                "--",
            ])?;
            let parent = parents.split_whitespace().next();
            changed(read.files(parent, Some(hash))?, path, parent, Some(hash))
        }
        DiffKey::Range { from, to, path } => changed(
            read.files(Some(from), to.as_deref())?,
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
                at(
                    Some(""),
                    if matches!(file.code, 'R' | 'C') {
                        file.old_path.as_deref().unwrap_or(&file.path)
                    } else {
                        &file.path
                    },
                ),
                (file.code != 'D').then(|| at(None, &file.path)).flatten(),
            )),
            Area::Untracked => Ok((None, at(None, &file.path))),
            Area::Conflicted => Err("Resolve the conflict before comparing its syntax.".to_owned()),
        },
    }
}

fn read_side(read: &Read<'_>, side: At) -> Result<String, String> {
    let Some((rev, path)) = side else {
        return Ok(String::new());
    };
    let bytes = match rev {
        Some(rev) => {
            let object = if rev.is_empty() { format!(":0:{path}") } else { format!("{rev}:{path}") };
            // Resolve once so an index/HEAD move between the size check and read cannot
            // turn a small blob into an unbounded one.
            let blob = read.text(&["rev-parse", "--verify", "--end-of-options", &object])?;
            let blob = blob.trim();
            let size = read.text(&["cat-file", "-s", "--", blob])?;
            if size.trim().parse::<usize>().map_err(|_| "Couldn't read this blob's size")? > MAX_BYTES {
                return Err("Syntax comparison is limited to 2 MiB per file.".to_owned());
            }
            read.bytes(&["cat-file", "blob", blob], MAX_BYTES)?
        }
        None => crate::safe_read::read_inside(read.repo, &path, MAX_BYTES as u64)
            .ok_or("Couldn't read a regular file within 2 MiB from the working tree (links are not followed).")?,
    };
    if bytes.len() > MAX_BYTES {
        return Err("Syntax comparison is limited to 2 MiB per file.".to_owned());
    }
    if bytes.contains(&0) {
        return Err("Syntax comparison is available for text files only.".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "Syntax comparison needs UTF-8 text.".to_owned())
}
