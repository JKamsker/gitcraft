//! What a pull request changes, as the units a reviewer reads: each hunk of its diff, and each
//! changed file that has no hunk to show (a rename alone, a binary, one too large to read).
//! Together they cover every change exactly once, in the order Git lists them.
//!
//! The diff and the repeated-edit patterns are those of the Repeated edits insight (#180):
//! [`crate::insights::transform`] reads the hunks and groups their edits.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::git::ChangedFile;
use crate::insights::transform::{self, Limits, Shape, Subject, Transform, diff};

/// What a unit is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) enum Body {
    /// A hunk: its lines in the old and the new version.
    Hunk {
        old_start: u32,
        old_len: u32,
        new_start: u32,
        new_len: u32,
    },
    /// A whole file, not split into hunks, and why.
    File(Whole),
}

/// Why a changed file is one unit rather than its hunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) enum Whole {
    /// Nothing in its lines changed: a rename, a copy or its mode alone.
    NoLines,
    Binary,
    /// More lines changed than are read.
    TooLarge,
    /// Beyond the limits of what is read (files or hunks).
    NotRead,
}

impl Whole {
    pub(in crate::repo::insights) fn label(self) -> &'static str {
        match self {
            Whole::NoLines => "no lines changed",
            Whole::Binary => "binary file",
            Whole::TooLarge => "too many lines changed to split into hunks",
            Whole::NotRead => "beyond the hunks read: shown whole",
        }
    }
}

/// What a hunk's lines say about it, for the rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo::insights) struct Markers {
    /// A public declaration it adds or removes, as written.
    pub(in crate::repo::insights) api: Option<String>,
    /// A test marker it adds or removes, as written.
    pub(in crate::repo::insights) test: Option<String>,
    /// Only whitespace or line breaks change in it.
    pub(in crate::repo::insights) layout: bool,
}

/// A part of the change a reviewer reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Unit {
    /// Who it is across readings of the same pull request: its file and its content, not its
    /// line numbers (which a rebase moves). Unique within a reading.
    pub(in crate::repo::insights) key: String,
    pub(in crate::repo::insights) path: String,
    pub(in crate::repo::insights) old_path: Option<String>,
    /// `A`, `M`, `D`, `R`…
    pub(in crate::repo::insights) status: char,
    pub(in crate::repo::insights) body: Body,
    pub(in crate::repo::insights) added: u32,
    pub(in crate::repo::insights) removed: u32,
    /// Its first changed line, to know it by.
    pub(in crate::repo::insights) preview: String,
    pub(in crate::repo::insights) markers: Markers,
    /// The repeated edit it makes, an index into [`Units::repeated`]; and whether it deviates.
    pub(in crate::repo::insights) repeated: Option<(usize, bool)>,
}

/// An edit the Repeated edits insight found made in several files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo::insights) struct Repeated {
    pub(in crate::repo::insights) label: String,
    pub(in crate::repo::insights) files: usize,
}

/// A pull request's units, read at two revisions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::repo::insights) struct Units {
    pub(in crate::repo::insights) base: String,
    pub(in crate::repo::insights) head: String,
    pub(in crate::repo::insights) units: Vec<Unit>,
    pub(in crate::repo::insights) repeated: Vec<Repeated>,
    /// What was read only in part, in words.
    pub(in crate::repo::insights) notes: Vec<String>,
    /// The grouping rules of the repository at the base, if it has any.
    pub(in crate::repo::insights) config: Option<String>,
}

/// Where the repository keeps its own grouping rules.
pub(in crate::repo::insights) const CONFIG: &str = ".gitcraft/review-groups";

/// Reads the units from `base` to `head`; `files` are the files the pull request changes.
pub(in crate::repo::insights) fn read(
    repo: &Path,
    base: &str,
    head: &str,
    files: &[ChangedFile],
) -> Result<Units, String> {
    let limits = Limits::default();
    let read = diff::read(repo, base, head, &limits)?;
    let subject = Subject {
        base: base.to_owned(),
        head: head.to_owned(),
        commit: None,
        scope: "the pull request since its merge base".to_owned(),
        tips: Vec::new(),
    };
    let patterns = transform::analyze(subject, read.clone(), &limits);
    let config = crate::git::run(Some(repo), &["show", &format!("{base}:{CONFIG}")]).ok();
    let mut units = build(base, head, files, &read, &patterns);
    units.config = config;
    Ok(units)
}

/// The units of what was read: its hunks, then each file of `files` without a hunk.
pub(in crate::repo::insights) fn build(
    base: &str,
    head: &str,
    files: &[ChangedFile],
    read: &diff::Read,
    patterns: &Transform,
) -> Units {
    let mut hunks: Vec<(usize, &diff::Hunk)> = read.hunks.iter().enumerate().collect();
    let mut notes = Vec::new();
    // The file read last may be cut short among its hunks: it is shown whole instead.
    let mut cut_file = None;
    if read.hunks_cut
        && let Some((_, last)) = hunks.last()
    {
        let path = last.path.clone();
        hunks.retain(|(_, h)| h.path != path);
        cut_file = Some(path);
    }
    if read.hunks_cut || read.beyond > 0 {
        notes.push("some files are past the limits of what is split into hunks".to_owned());
    }
    let (repeated, by_hunk, layout) = repeated_of(patterns);
    let of_file: HashMap<&str, &ChangedFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    let order: HashMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.path.as_str(), i))
        .collect();
    // In the pull request's order of files; hunks of a file in their own order.
    hunks.sort_by_key(|(i, h)| (order.get(h.path.as_str()).copied().unwrap_or(usize::MAX), *i));
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut by_path: HashMap<&str, Vec<Unit>> = HashMap::new();
    let mut strays = Vec::new();
    for (index, h) in hunks {
        let file = of_file.get(h.path.as_str());
        let digest = format!("{:x}", md5::compute(format!("{:?}{:?}", h.removed, h.added)));
        let n = seen.entry(format!("{}\0{digest}", h.path)).or_insert(0);
        *n += 1;
        let unit = Unit {
            key: format!("h:{}:{digest}:{n}", h.path),
            path: h.path.clone(),
            old_path: file.and_then(|f| f.old_path.clone()),
            status: file.map_or('M', |f| f.status),
            body: Body::Hunk {
                old_start: h.old_start,
                old_len: h.old_len,
                new_start: h.new_start,
                new_len: h.new_len,
            },
            added: h.added.len() as u32,
            removed: h.removed.len() as u32,
            preview: preview(h),
            markers: markers(h, layout.contains(&index)),
            repeated: by_hunk.get(&index).copied(),
        };
        match file {
            Some(f) => by_path.entry(f.path.as_str()).or_default().push(unit),
            None => strays.push(unit),
        }
    }
    let mut units = Vec::new();
    for f in files {
        match by_path.remove(f.path.as_str()) {
            Some(list) => units.extend(list),
            None => {
                let why = if cut_file.as_deref() == Some(f.path.as_str()) {
                    Whole::NotRead
                } else if read.too_large.contains(&f.path) {
                    Whole::TooLarge
                } else if f.added.is_none() {
                    Whole::Binary
                } else if f.added.unwrap_or(0) + f.removed.unwrap_or(0) == 0 {
                    Whole::NoLines
                } else {
                    Whole::NotRead
                };
                units.push(whole(f, why));
            }
        }
    }
    units.extend(strays);
    Units {
        base: base.to_owned(),
        head: head.to_owned(),
        units,
        repeated,
        notes,
        config: None,
    }
}

fn whole(f: &ChangedFile, why: Whole) -> Unit {
    Unit {
        key: format!("f:{}", f.path),
        path: f.path.clone(),
        old_path: f.old_path.clone(),
        status: f.status,
        body: Body::File(why),
        added: f.added.unwrap_or(0),
        removed: f.removed.unwrap_or(0),
        preview: String::new(),
        markers: Markers::default(),
        repeated: None,
    }
}

/// What the patterns of the Repeated edits insight say of each hunk.
type Found = (Vec<Repeated>, HashMap<usize, (usize, bool)>, HashSet<usize>);

/// The edits made in more than one file (whitespace and large edits aside); for each hunk
/// whose edits all make one of them (or deviate from one), which one; and the hunks whose
/// edits change only whitespace or line breaks.
fn repeated_of(t: &Transform) -> Found {
    let qualifies = |p: usize| {
        let pattern = &t.patterns[p];
        pattern.deviates_from.is_none()
            && pattern.edits.len() > 1
            && pattern.per_file.len() > 1
            && !matches!(pattern.shape, Shape::Layout | Shape::Large)
    };
    let mut index = HashMap::new();
    let mut repeated = Vec::new();
    for p in (0..t.patterns.len()).filter(|&p| qualifies(p)) {
        index.insert(p, repeated.len());
        repeated.push(Repeated {
            label: t.patterns[p].label(),
            files: t.patterns[p].per_file.len(),
        });
    }
    // Each hunk's (pattern made, deviating) per edit; a hunk mixing several isn't one edit.
    let mut per_hunk: HashMap<usize, Option<(usize, bool)>> = HashMap::new();
    let mut layout: HashMap<usize, bool> = HashMap::new();
    for (p, pattern) in t.patterns.iter().enumerate() {
        let made = match pattern.deviates_from {
            Some(parent) => index.get(&parent).map(|&r| (r, true)),
            None => index.get(&p).map(|&r| (r, false)),
        };
        for &e in &pattern.edits {
            let hunk = t.edits[e].hunk;
            *layout.entry(hunk).or_insert(true) &= pattern.shape == Shape::Layout;
            let slot = per_hunk.entry(hunk).or_insert(made);
            *slot = match (*slot, made) {
                (Some((a, da)), Some((b, db))) if a == b => Some((a, da || db)),
                _ => None,
            };
        }
    }
    let by_hunk = per_hunk
        .into_iter()
        .filter_map(|(h, m)| m.map(|m| (h, m)))
        .collect();
    let layout = layout.into_iter().filter(|&(_, l)| l).map(|(h, _)| h).collect();
    (repeated, by_hunk, layout)
}

fn preview(h: &diff::Hunk) -> String {
    let line = h
        .added
        .iter()
        .map(|l| ("+", l))
        .chain(h.removed.iter().map(|l| ("−", l)))
        .find(|(_, l)| !l.trim().is_empty());
    match line {
        Some((sign, l)) => {
            let text: String = l.trim().chars().take(80).collect();
            format!("{sign} {text}")
        }
        None => String::new(),
    }
}

/// Starts of lines declaring something other code can use.
const API: [&str; 9] = [
    "pub fn ",
    "pub struct ",
    "pub enum ",
    "pub trait ",
    "pub type ",
    "pub const ",
    "export ",
    "public ",
    "interface ",
];

/// What marks a test.
const TEST: [&str; 7] = [
    "#[test]",
    "#[cfg(test)]",
    "#[tokio::test]",
    "@Test",
    "def test_",
    "func Test",
    "describe(",
];

fn markers(h: &diff::Hunk, layout: bool) -> Markers {
    let lines = || h.added.iter().chain(&h.removed).map(|l| l.trim());
    let api = lines()
        .find(|l| {
            let l = l.strip_prefix("async ").unwrap_or(l);
            API.iter().any(|a| l.starts_with(a))
        })
        .map(|l| l.chars().take(60).collect());
    let test = lines()
        .find_map(|l| TEST.iter().find(|t| l.contains(*t)))
        .map(|t| (*t).to_owned());
    Markers { api, test, layout }
}
