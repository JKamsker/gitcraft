//! Central pane while a file is open: its diff, unified or side by side, with syntax colours,
//! the changed part of each line marked, images shown before / after and files in Git LFS as
//! their objects. Uncommitted changes can be staged, unstaged or discarded a hunk or a few
//! lines at a time.

mod algorithm;
#[cfg(test)]
mod algorithm_tests;
mod binary;
mod image_compare;
mod images;
mod interdiff;
mod jump;
mod lfs;
pub(in crate::repo) mod link;
mod lines;
pub(super) mod line_diff;
mod map;
mod paint;
mod parse;
mod patch;
mod remerge;
mod place;
pub(super) mod rendered;
mod rows;
mod select;
#[cfg(test)]
mod tests;
mod view;
mod weakened;

use std::path::Path;

use super::DiffKey;
use super::highlight::{self, MAX_LINE_CHARS, Spans, cap_line};
use crate::git::{self, Area, DiffOptions};

pub(super) use algorithm::algorithm_for;
pub use images::ImagePair;
use images::{is_image, load_images};
pub(in crate::repo) use interdiff::{interdiff_data, interdiff_rows};
pub use lfs::LfsPair;
use lfs::lfs_pair;
pub use lines::{old_name, picked_in_file, picked_lines};
pub use parse::parse;
pub(in crate::repo) use parse::patch_header;
use parse::{is_new_file, pair_lines};
pub use patch::{hunk_changes, partial_patch};
pub(super) use place::{Place, offset_of_place};
pub(super) use select::TextSel;

const LINE_H: f32 = 19.0;
const NUM_W: f32 = 46.0;
/// Width of the strip of changes beside the scrollbar.
const MAP_W: f32 = 12.0;
/// Very long diffs are cut here; the UI stays responsive and git is one click away.
const MAX_LINES: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Context,
    Add,
    Del,
    Hunk,
    /// Binary notices, "No newline at end of file".
    Meta,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: Kind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    /// What is shown: the line without its `+`/`-` column, tabs expanded.
    pub text: String,
    /// The line exactly as git printed it, to build patches from.
    pub raw: String,
}

/// Everything the view needs, prepared on a background thread.
pub struct DiffData {
    pub lines: Vec<DiffLine>,
    /// Syntax colours per line (`None` when the file type isn't known).
    pub spans: Vec<Option<Spans>>,
    /// The changed part of a removed / added line paired with its counterpart (char range).
    pub changed: Vec<Option<(usize, usize)>>,
    /// Side-by-side rows: (left line, right line).
    pub split: Vec<(Option<usize>, Option<usize>)>,
    pub image: Option<ImagePair>,
    /// The two versions of the picture decoded, to compare in every mode; `None` when they
    /// don't decode (they show as `image` then).
    pub compare: Option<image_compare::Compare>,
    /// The file is kept in Git LFS: the objects before and after, shown instead of the lines.
    pub lfs: Option<LfsPair>,
    /// A binary file (not an image, not in LFS): both versions, shown instead of git's
    /// "Binary files differ".
    pub binary: Option<binary::BinaryPair>,
    /// The program git turned the file into text with (`textconv`), when it did.
    pub textconv: Option<String>,
    /// The patch's file header (`diff --git`, `index`, `---`, `+++`), for partial patches.
    pub header: String,
    /// Hunks and lines can be staged, unstaged or discarded on their own.
    pub partial: bool,
    /// Hunks and lines of a commit's diff can be reverted on their own.
    pub revertable: bool,
    /// The file is new: it has no old side, so it shows unified even side by side.
    pub new_file: bool,
    /// The lines added and removed; `None` when there are none to count (a binary file, one
    /// git doesn't diff), as `--numstat` says of those.
    pub line_counts: (Option<u32>, Option<u32>),
    /// Characters in the longest line, for the width to scroll (counted once, not every frame).
    longest: usize,
    /// The lines cut to be shown, as they were: what copying them copies.
    pub(super) uncut: std::collections::HashMap<usize, String>,
    /// The runs of changed rows and the changes they make, unified and side by side: worked
    /// out the first time they are needed, not every frame.
    /// The _weakens checks_ evidence of each hunk that has some, by its header's line.
    pub(super) weakened: std::collections::HashMap<usize, String>,
    map: [std::sync::OnceLock<(map::ChangeRuns, Vec<std::ops::Range<usize>>)>; 2],
}

impl DiffData {
    /// Whether the lines show side by side when that's the setting (`split`): a new file takes
    /// the whole width, rather than half of it next to an empty old side.
    pub(in crate::repo) fn shows_split(&self, split: bool) -> bool {
        split && !self.new_file
    }

    /// The runs of changed rows shown unified or side by side (`split`), and the changes they
    /// make (see [`jump::changes`]).
    pub(in crate::repo::diff) fn runs(
        &self,
        split: bool,
    ) -> &(map::ChangeRuns, Vec<std::ops::Range<usize>>) {
        self.map[usize::from(split)].get_or_init(|| {
            let runs = map::runs_of(self, split);
            let changes = jump::changes(&runs);
            (runs, changes)
        })
    }
}

/// What to do with some hunks or lines of an uncommitted file's diff, or of a commit's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Unstaged lines into the index.
    Stage,
    /// Staged lines out of the index.
    Unstage,
    /// Unstaged lines out of the working tree.
    Discard,
    /// A commit's lines undone in the working tree.
    Revert,
}

impl Pick {
    /// The patch is applied with `--reverse`.
    fn reverse(self) -> bool {
        self != Pick::Stage
    }

    /// `git apply` arguments before the patch file.
    pub fn args(self) -> &'static [&'static str] {
        match self {
            Pick::Stage => &["apply", "--cached"],
            Pick::Unstage => &["apply", "--cached", "--reverse"],
            Pick::Discard | Pick::Revert => &["apply", "--reverse"],
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Pick::Stage => "Stage",
            Pick::Unstage => "Unstage",
            Pick::Discard => "Discard",
            Pick::Revert => "Revert",
        }
    }
}

/// [`load`], or with `lines` only the part of a commit's diff that changed those lines of the
/// file (as they are in the commit), as `git log -L` shows it.
pub fn load_lines(
    repo: &Path,
    key: &DiffKey,
    lines: Option<&[git::LineRange]>,
    opts: DiffOptions,
    dark: bool,
) -> Result<DiffData, String> {
    match (lines, key) {
        (Some(ranges), DiffKey::Commit { hash, path }) => {
            let text = git::line_diff(repo, hash, path, ranges)?;
            // Not the commit's patch: nothing of it is picked to revert.
            Ok(prepare(repo, key, path, &text, false, dark))
        }
        _ => load(repo, key, opts, dark),
    }
}

/// Reads and prepares the diff for `key`.
pub fn load(repo: &Path, key: &DiffKey, opts: DiffOptions, dark: bool) -> Result<DiffData, String> {
    let patch = |opts: DiffOptions| -> Result<(String, String), String> {
        Ok(match key {
            DiffKey::Commit { hash, path } => {
                (git::file_diff(repo, hash, path, opts)?, path.clone())
            }
            DiffKey::Work(file) => (git::work_diff(repo, file, opts)?, file.path.clone()),
            DiffKey::Range { from, to, path } => (
                git::range_file_diff(repo, from, to.as_deref(), path, opts)?,
                path.clone(),
            ),
        })
    };
    // A textconv driver that fails (Git for Windows maps `*.docx` and `*.pdf` to its own
    // `astextplain`, which refuses some files) fails the whole diff: the file's own bytes
    // are diffed instead, so it is shown as a binary rather than not at all.
    let (text, path) = match patch(opts) {
        Err(e) if !opts.no_textconv => patch(DiffOptions {
            no_textconv: true,
            ..opts
        })
        .map_err(|_| e)?,
        done => done?,
        DiffKey::Remerge {
            hash,
            path,
            old_path,
            ..
        } => (
            git::remerge_file_diff(repo, hash, path, old_path.as_deref(), opts)?,
            path.clone(),
        ),
    };
    let pickable = !opts.ignore_ws;
    Ok(prepare(repo, key, &path, &text, pickable, dark))
}

/// The diff `text` of `path` read into lines, coloured and paired. `pickable`: hunks and lines
/// of it can be picked on their own (it is the patch git would apply).
fn prepare(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    text: &str,
    pickable: bool,
    dark: bool,
) -> DiffData {
    let path = path.to_owned();
    let mut lines = parse(text);
    if let DiffKey::Remerge { conflicts, .. } = key {
        lines = remerge::label(conflicts, lines, !pickable);
    }
    let header = patch_header(text);
    let picks = pickable && can_pick(&header, &lines);
    let partial =
        picks && matches!(key, DiffKey::Work(f) if matches!(f.area, Area::Staged | Area::Unstaged));
    // Not a submodule's change, which `git apply` would report done and leave as it was.
    let revertable = picks
        && matches!(key, DiffKey::Commit { .. })
        && git::check_revertable(text, &path).is_ok();
    let new_file = is_new_file(text);
    let lfs = lfs_pair(&lines);
    // An image in LFS: git has only its pointer.
    let image = if is_image(&path) && lfs.is_none() {
        Some(load_images(repo, key, &path, images::renamed_from(text)))
    } else {
        None
    };
    let binary = (image.is_none() && lfs.is_none() && binary::is_binary_patch(&text))
        .then(|| binary::load_pair(repo, key, &path, renamed_from(&text)));
    // A diff of what the file's textconv made of it: its lines aren't the file's. Not for a
    // symbolic link: git diffs those with its default driver, whatever the attribute says.
    let textconv = (binary.is_none() && !lines.is_empty() && !is_symlink_patch(&text))
        .then(|| binary::textconv_for(repo, &path))
        .flatten();
    let partial = partial && textconv.is_none();
    let compare = image
        .as_ref()
        .and_then(|pair| image_compare::compare(repo, key, &path, pair));
    let mut spans = highlight_sides(&path, &lines, dark);
    let (mut changed, split) = pair_lines(&lines);
    let mut uncut = std::collections::HashMap::new();
    for (i, (line, spans)) in lines.iter_mut().zip(spans.iter_mut()).enumerate() {
        if let Some(whole) = cap_line(&mut line.text, spans.as_mut()) {
            uncut.insert(i, whole);
        }
    }
    for range in changed.iter_mut().flatten() {
        *range = (range.0.min(MAX_LINE_CHARS), range.1.min(MAX_LINE_CHARS));
    }
    let longest = lines
        .iter()
        .map(|l| l.text.chars().count())
        .max()
        .unwrap_or(0);
    let line_counts = count_lines(&lines);
    let weakened = weakened::by_hunk_line(&text, &lines);
    DiffData {
        line_counts,
        lines,
        spans,
        changed,
        split,
        image,
        compare,
        lfs,
        binary,
        textconv,
        header,
        partial,
        revertable,
        new_file,
        longest,
        uncut,
        weakened,
        map: Default::default(),
    }
}

/// Whether the patch is of a symbolic link (mode 120000 before its first hunk, no file mode).
pub(super) fn is_symlink_patch(text: &str) -> bool {
    let modes: Vec<&str> = text
        .split_terminator('\n')
        .take_while(|l| !l.starts_with("@@"))
        .filter(|l| l.starts_with("index ") || l.contains(" mode "))
        .filter_map(|l| l.rsplit(' ').next())
        .filter(|m| m.len() == 6 && m.bytes().all(|c| c.is_ascii_digit()))
        .collect();
    modes.contains(&"120000") && modes.iter().all(|m| *m == "120000")
}

/// The lines `lines` adds and removes; `None` when it has none to count (a binary file, one
/// git doesn't diff).
fn count_lines(lines: &[DiffLine]) -> (Option<u32>, Option<u32>) {
    if lines.iter().all(|l| l.kind == Kind::Meta) {
        return (None, None);
    }
    let count = |kind| {
        let n = lines.iter().filter(|l| l.kind == kind).count();
        Some(u32::try_from(n).unwrap_or(u32::MAX))
    };
    (count(Kind::Add), count(Kind::Del))
}

/// Whether hunks and lines of this diff can be picked on their own (it isn't a `-w` diff,
/// whose patch wouldn't apply: see [`prepare`]).
fn can_pick(header: &str, lines: &[DiffLine]) -> bool {
    // New, deleted, renamed and binary files, truncated diffs and text that isn't UTF-8 only
    // go whole.
    !header.is_empty()
        && ![
            "new file mode",
            "deleted file mode",
            "rename from",
            "copy from",
            "Binary files",
        ]
        .iter()
        .any(|h| header.contains(h))
        && lines
            .iter()
            .all(|l| l.kind != Kind::Meta || l.raw.starts_with('\\'))
        // Not UTF-8 (a Latin-1 file…): the text was read with its odd bytes replaced, and a
        // patch made from it would write the replacements into the added lines.
        && !lines.iter().any(|l| l.raw.contains(char::REPLACEMENT_CHARACTER))
}

/// Syntax colours per line. The old and the new side are each coloured as one stream, so
/// multi-line constructs work.
fn highlight_sides(path: &str, lines: &[DiffLine], dark: bool) -> Vec<Option<Spans>> {
    let mut spans = vec![None; lines.len()];
    let old_side: Vec<usize> = (0..lines.len())
        .filter(|&i| matches!(lines[i].kind, Kind::Context | Kind::Del))
        .collect();
    let new_side: Vec<usize> = (0..lines.len())
        .filter(|&i| matches!(lines[i].kind, Kind::Context | Kind::Add))
        .collect();
    if let Some(old) =
        highlight::highlight(path, old_side.iter().map(|&i| lines[i].text.as_str()), dark)
    {
        for (k, &i) in old_side.iter().enumerate() {
            if lines[i].kind == Kind::Del {
                spans[i] = old.get(k).cloned();
            }
        }
    }
    if let Some(new) =
        highlight::highlight(path, new_side.iter().map(|&i| lines[i].text.as_str()), dark)
    {
        for (k, &i) in new_side.iter().enumerate() {
            spans[i] = new.get(k).cloned();
        }
    }
    spans
}

/// `git restore --staged`, or `git rm --cached` before the first commit (no HEAD to restore from).
/// The path is taken literally, with `:(literal)` rather than `--literal-pathspecs`, which
/// `restore` would pass on to the post-checkout hook.
pub(super) fn unstage_args(head: &str, path: &str) -> Vec<String> {
    let path = literal_path(path);
    let args: &[&str] = if head.is_empty() {
        &["rm", "--cached", "-q", "--", &path]
    } else {
        &["restore", "--staged", "--", &path]
    };
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// `path` as a pathspec that is only that file name, not a pattern (`[ab].txt`, `*`, `:x`).
/// For commands that run hooks, which would inherit `--literal-pathspecs` (and with it change
/// how the hook's own git commands read their patterns).
pub(in crate::repo) fn literal_path(path: &str) -> String {
    format!(":(literal){path}")
}
