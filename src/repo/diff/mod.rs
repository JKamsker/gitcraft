//! Central pane while a file is open: its diff, unified or side by side, with syntax colours,
//! the changed part of each line marked, images shown before / after and files in Git LFS as
//! their objects. Uncommitted changes can be staged, unstaged or discarded a hunk or a few
//! lines at a time.

mod algorithm;
#[cfg(test)]
mod algorithm_tests;
mod binary;
pub(super) mod context;
pub(in crate::repo) mod encrypted;
mod fold_view;
mod image_compare;
mod images;
mod interdiff;
mod jump;
mod lfs;
pub(super) mod line_diff;
mod lines;
pub(in crate::repo) mod link;
mod map;
pub(super) mod moved;
#[cfg(test)]
mod moved_tests;
pub(super) mod moved_view;
mod notebook;
mod paint;
mod parse;
mod patch;
mod place;
mod presentation;
mod remerge;
pub(super) mod rendered;
pub(super) mod rich;
mod rows;
mod select;
pub(in crate::repo) mod structural;
#[cfg(test)]
mod tests;
mod view;
mod weakened;
mod wrap;

#[cfg(test)]
pub(super) use binary::{make_docx, make_pdf, make_zip};
#[cfg(test)]
pub(super) use image_compare::cpu as cpu_images;

use std::path::Path;
use std::time::Instant;

use super::DiffKey;
use super::highlight::{self, MAX_LINE_CHARS, Spans, cap_line};
use crate::git::{self, Area, DiffOptions};

pub(super) use algorithm::algorithm_for;
pub use context::Unfold;
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
/// The +/− column selects Git action lines separately from numbers and source text.
const MARK_W: f32 = 24.0;
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
    /// The unchanged lines after the last hunk, folded (see [`context`]).
    Fold,
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
#[derive(Default)]
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
    /// A Jupyter notebook: its cells compared, shown instead of its JSON unless that is picked.
    pub notebook: Option<notebook::NotebookData>,
    /// The patch's file header (`diff --git`, `index`, `---`, `+++`), for partial patches.
    pub header: String,
    /// Hunks and lines can be staged, unstaged or discarded on their own.
    pub partial: bool,
    pub sops: bool,
    /// Hunks and lines of a commit's diff can be reverted on their own.
    pub revertable: bool,
    /// Blocks removed in one place and added in another.
    pub moves: moved::Moves,
    /// The unchanged lines between the hunks, folded, and the file they open from.
    pub context: Option<context::Context>,
    /// The file is new: it has no old side, so it shows unified even side by side.
    pub new_file: bool,
    /// The lines added and removed; `None` when there are none to count (a binary file, one
    /// git doesn't diff), as `--numstat` says of those.
    pub line_counts: (Option<u32>, Option<u32>),
    /// Characters in the longest line, for the width to scroll (counted once, not every frame).
    longest: usize,
    /// A hash of the diff text git gave: two reads with the same one show the same change.
    pub(super) source: u64,
    /// Identity of row mutations, independent of the original diff text and row count.
    pub(super) geometry_revision: u64,
    /// The lines cut to be shown, as they were: what copying them copies.
    pub(super) uncut: std::collections::HashMap<usize, String>,
    /// The runs of changed rows and the changes they make, unified and side by side: worked
    /// out the first time they are needed, not every frame.
    /// The _weakens checks_ evidence of each hunk that has some, by its header's line.
    pub(super) weakened: std::collections::HashMap<usize, weakened::HunkSummary>,
    /// Who wrote each added line, as its commit records (a commit's diff only).
    pub(super) authors: Vec<Option<crate::authorship::Who>>,
    hunks: [std::sync::OnceLock<Vec<(usize, usize)>>; 2],
    map: [std::sync::OnceLock<(map::ChangeRuns, Vec<std::ops::Range<usize>>)>; 2],
}

impl DiffData {
    pub(in crate::repo) fn hunk_at(&self, split: bool, row: usize) -> Option<usize> {
        let headers = self.hunks[usize::from(split)].get_or_init(|| {
            let rows = presentation::DisplayRows::new(self, split);
            (0..rows.len())
                .filter_map(|row| {
                    let (left, right) = rows.patch_lines(row)?;
                    let line = left.or(right)?;
                    (self.lines[line].kind == Kind::Hunk).then_some((row, line))
                })
                .collect()
        });
        let at = headers
            .partition_point(|(start, _)| *start <= row)
            .saturating_sub(1);
        headers.get(at).map(|(_, header)| *header)
    }

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

/// Pair raw patch lines for the snapshot-only file comparison panes.
pub(in crate::repo) fn comparison_pairs(lines: &[DiffLine]) -> Vec<(Option<usize>, Option<usize>)> {
    pair_lines(lines).1
}

pub(in crate::repo) use wrap::logical_offset as logical_wrap_offset;

mod load;
#[cfg(test)]
use load::{find_moves, is_symlink_patch, prepare};
pub use load::{load, load_lines};
