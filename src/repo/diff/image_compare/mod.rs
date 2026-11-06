//! Comparing the two versions of a picture: 2-up, Swipe, Onion skin and Difference, with zoom
//! and pan linked across them, a checkerboard behind transparency, what changed in size and
//! dimensions, and the picture's history as a strip of thumbnails.

mod decode;
mod filmstrip;
mod view;
mod view_state;

use std::path::{Path, PathBuf};

pub(super) use view::compare_view;

use super::DiffKey;
use super::images::ImagePair;
use crate::git;
use decode::{Decoded, Difference};

/// How many of the picture's commits the strip shows.
const HISTORY: usize = 60;

/// One commit of the picture's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub hash: String,
    pub time: i64,
    pub summary: String,
    /// The picture's name in that commit (it may have been renamed since).
    pub path: String,
}

/// Both versions of a picture, decoded, and what the comparison needs.
pub struct Compare {
    pub old: Option<Decoded>,
    pub new: Option<Decoded>,
    pub diff: Option<Difference>,
    /// The sizes of the files.
    pub old_bytes: Option<usize>,
    pub new_bytes: Option<usize>,
    /// Names the textures by what they hold (see `images::image_id`).
    pub id: String,
    pub repo: PathBuf,
    pub path: String,
    /// The commits that changed the picture, newest first, and the one this diff is of.
    pub history: Vec<Version>,
    pub current: Option<String>,
}

/// The comparison of `pair` (read for `key`), when both sides that are there decode; else
/// `None`, and the pictures show as they did before.
pub(super) fn compare(repo: &Path, key: &DiffKey, path: &str, pair: &ImagePair) -> Option<Compare> {
    let decode = |b: &Option<std::sync::Arc<[u8]>>| match b {
        Some(b) => decode::decode(b).map(Some),
        None => Ok(None),
    };
    let (old, new) = (decode(&pair.old).ok()?, decode(&pair.new).ok()?);
    if old.is_none() && new.is_none() {
        return None;
    }
    let diff = match (&old, &new) {
        (Some(a), Some(b)) => Some(decode::difference(&a.image, &b.image)),
        _ => None,
    };
    let (from, current) = match key {
        DiffKey::Commit { hash, .. } => (Some(hash.as_str()), Some(hash.clone())),
        DiffKey::Range { to: Some(to), .. } => (Some(to.as_str()), None),
        _ => (None, None),
    };
    Some(Compare {
        old,
        new,
        diff,
        old_bytes: pair.old.as_ref().map(|b| b.len()),
        new_bytes: pair.new.as_ref().map(|b| b.len()),
        id: pair.id.clone(),
        repo: repo.to_path_buf(),
        path: path.to_owned(),
        history: history(repo, from, path).unwrap_or_default(),
        current,
    })
}

/// The commits that changed `path` up to `from` (HEAD when `None`), newest first, following
/// renames, each with the picture's name there.
pub(super) fn history(repo: &Path, from: Option<&str>, path: &str) -> Result<Vec<Version>, String> {
    let n = format!("-n{HISTORY}");
    let mut args = vec![
        "--literal-pathspecs",
        "-c",
        "core.quotePath=false",
        "log",
        "--no-color",
        "--format=%x1e%H%x1f%ct%x1f%s",
        "--name-only",
        &n,
        "--follow",
    ];
    args.extend(from);
    args.extend(["--", path]);
    let out = git::run(Some(repo), &args)?;
    Ok(parse_history(&out, path))
}

/// `git log`'s records: `RS hash US time US summary`, a blank line, the file's name.
pub(super) fn parse_history(out: &str, path: &str) -> Vec<Version> {
    out.split('\x1e')
        .filter_map(|record| {
            let (head, names) = record.split_once('\n').unwrap_or((record, ""));
            let mut f = head.splitn(3, '\x1f');
            Some(Version {
                hash: f.next()?.trim().to_owned(),
                time: f.next()?.parse().ok()?,
                summary: f.next().unwrap_or("").to_owned(),
                path: names
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or(path)
                    .to_owned(),
            })
        })
        .filter(|v| !v.hash.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
