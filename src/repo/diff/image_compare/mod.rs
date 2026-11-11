//! Comparing the two versions of a picture: 2-up, Swipe, Onion skin and Difference, with zoom
//! and pan linked across them, a checkerboard behind transparency, what changed in size and
//! dimensions, and the picture's history as a strip of thumbnails.

mod decode;
mod filmstrip;
mod lazy;
mod view;
mod view_state;

#[cfg(test)]
pub(in crate::repo) mod cpu {
    pub(in crate::repo) use super::decode::{decode, difference, ready};
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(super) use view::compare_view;

use super::DiffKey;
use super::images::ImagePair;
use crate::git;
use decode::{Decoded, Difference};
use lazy::Lazy;

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
    /// Where they differ: worked out the first time it is shown.
    pub diff: Lazy<Option<Arc<Difference>>>,
    /// The sizes of the files.
    pub old_bytes: Option<usize>,
    pub new_bytes: Option<usize>,
    /// Names the textures by what they hold (see `images::image_id`).
    pub id: String,
    pub repo: PathBuf,
    pub path: String,
    /// The commits that changed the picture, newest first (read the first time the strip
    /// shows, up to `from`), and the one this diff is of.
    pub history: Lazy<Arc<Vec<Version>>>,
    pub from: Option<String>,
    pub current: Option<String>,
    /// The textures made for it, let go of with it.
    pub textures: Mutex<HashMap<String, egui::TextureHandle>>,
}

impl Compare {
    /// Where the two pictures differ, once worked out (`None` until then, or with one side).
    pub fn difference(&self, ctx: &egui::Context) -> Option<Arc<Difference>> {
        let (a, b) = (
            self.old.as_ref()?.image.clone(),
            self.new.as_ref()?.image.clone(),
        );
        self.diff
            .get(
                ctx,
                move || Some(Arc::new(decode::difference(&a, &b))),
                None,
            )
            .flatten()
    }

    /// The picture's history, once read.
    pub fn versions(&self, ctx: &egui::Context) -> Option<Arc<Vec<Version>>> {
        let (repo, from, path) = (self.repo.clone(), self.from.clone(), self.path.clone());
        self.history.get(
            ctx,
            move || Arc::new(history(&repo, from.as_deref(), &path).unwrap_or_default()),
            Arc::new(Vec::new()),
        )
    }
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
    let (old, new) = decode::ready(old, new);
    let (from, current) = match key {
        DiffKey::Commit { hash, .. } => (Some(hash.clone()), Some(hash.clone())),
        DiffKey::Range { to: Some(to), .. } => (Some(to.clone()), None),
        _ => (None, None),
    };
    Some(Compare {
        old,
        new,
        diff: Lazy::default(),
        old_bytes: pair.old.as_ref().map(|b| b.len()),
        new_bytes: pair.new.as_ref().map(|b| b.len()),
        id: pair.id.clone(),
        repo: repo.to_path_buf(),
        path: path.to_owned(),
        history: Lazy::default(),
        from,
        current,
        textures: Mutex::default(),
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
