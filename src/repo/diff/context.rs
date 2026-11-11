//! The unchanged lines between hunks, folded: _42 unchanged lines_ on each hunk's header (and
//! a row of its own after the last hunk) opens upwards, downwards, all at once or as the
//! function the hunk is in. Lines opened join the hunk next to them as context, so a hunk or
//! the lines picked in it still stage as before (`git apply` only gets more context).

use std::path::Path;

use super::highlight::{self, Spans};
use super::{DiffData, DiffLine, Kind};
use crate::git::{self, Area};
use crate::repo::DiffKey;

/// Lines one click on _↑_ or _↓_ opens.
pub const STEP: u32 = 20;
/// A file bigger than this isn't read for its unchanged lines.
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
/// More lines than this opened at once are shown without syntax colours: colouring them would
/// hold up the window.
const MAX_COLOURED: usize = 2_000;
/// How far up from a hunk its function is looked for.
const MAX_FUNCTION_LINES: u32 = 5_000;

/// Unchanged lines of the new side not shown: above hunk `n` (the `n`th gap), or after the
/// last hunk (the trailing gap).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    /// The row it folds on: the header of the hunk below it, or the fold row after the last.
    pub row: usize,
    /// The first and last line of the new side not shown (none when `first > last`).
    pub first: u32,
    pub last: u32,
    /// What turns a line number of the new side into one of the old side, in this gap.
    pub old_offset: i64,
    /// After the last hunk: it opens downwards only, on a row of its own.
    pub trailing: bool,
    /// The first and last line of the function the hunk below is in, when git named one and it
    /// encloses the hunk.
    pub function: Option<(u32, u32)>,
}

impl Gap {
    /// How many lines it hides.
    pub fn hidden(&self) -> u32 {
        if self.last >= self.first {
            self.last - self.first + 1
        } else {
            0
        }
    }

    /// Whether it opens downwards (from the hunk above it): every gap but the one before the
    /// first hunk.
    pub fn opens_down(&self, index: usize) -> bool {
        index > 0 || self.trailing
    }
}

/// The new side of the file, where the unchanged lines are read from, and the gaps.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// The lines as git has them (a `\r` before the newline kept), without the newline.
    raw: Vec<String>,
    /// The file, for the syntax colours of the lines opened, and whether the theme is dark.
    path: String,
    dark: bool,
    /// The file's last line has no newline: git notes that after it.
    no_newline_at_end: bool,
    pub gaps: Vec<Gap>,
}

/// How a gap opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unfold {
    /// The lines right above the hunk below the gap.
    Up(u32),
    /// The lines right below the hunk above it.
    Down(u32),
    All,
    /// As far as the function the hunk below the gap is in, both ways.
    Function,
}

/// Where the new side of `key`'s file is (`""`: the index; `None`: on disk), when the diff has
/// unchanged lines to fold: a new or untracked file is all added, a conflict's diff combined.
pub fn new_side(key: &DiffKey) -> Option<Option<String>> {
    match key {
        DiffKey::Remerge { .. } => None,
        DiffKey::Commit { hash, .. } => Some(Some(hash.clone())),
        DiffKey::Range { to, .. } => Some(to.clone()),
        DiffKey::Work(file) => match file.area {
            Area::Staged => Some(Some(String::new())),
            Area::Unstaged => Some(None),
            Area::Untracked | Area::Conflicted => None,
        },
    }
}

/// The hunk header's numbers: (old start, old count, new start, new count) and what follows.
pub(super) fn header_numbers(header: &str) -> Option<(u32, u32, u32, u32, &str)> {
    let rest = header.strip_prefix("@@ -")?;
    let (ranges, tail) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |r: &str| -> Option<(u32, u32)> {
        match r.split_once(',') {
            Some((s, n)) => Some((s.parse().ok()?, n.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (os, on) = range(old)?;
    let (ns, nn) = range(new)?;
    Some((os, on, ns, nn, tail))
}

/// The line before a range's first one (git numbers an empty range by that line).
fn before(start: u32, count: u32) -> u32 {
    if count == 0 {
        start
    } else {
        start.saturating_sub(1)
    }
}

/// Reads the new side of the file of `lines` (one file's diff) and finds the gaps.
pub fn read(repo: &Path, key: &DiffKey, lines: &[DiffLine], dark: bool) -> Option<Context> {
    // Nothing to fold without hunks (a binary file, a mode change).
    if !lines.iter().any(|l| l.kind == Kind::Hunk) {
        return None;
    }
    let rev = new_side(key)?;
    let path = key.path();
    let bytes = git::file_at(repo, rev.as_deref(), path).ok()?;
    if bytes.len() > MAX_FILE_BYTES {
        return None;
    }
    // Not UTF-8: lines read with their odd bytes replaced would be wrong context to stage with.
    let text = String::from_utf8(bytes).ok()?;
    let raw: Vec<String> = text.split_terminator('\n').map(str::to_owned).collect();
    // What git diffed must be this file as read: not so after a clean filter or textconv
    // (`core.autocrlf`, Git LFS), or when the file changed since. Context lines that don't
    // match would make wrong patches.
    let matches = lines.iter().all(|l| {
        l.kind != Kind::Context
            || l.new.is_some_and(|n| {
                n >= 1 && raw.get(n as usize - 1).map(String::as_str) == l.raw.get(1..)
            })
    });
    if !matches {
        return None;
    }
    let len = raw.len() as u32;
    let mut ctx = Context {
        raw,
        path: path.to_owned(),
        dark,
        no_newline_at_end: !text.is_empty() && !text.ends_with('\n'),
        gaps: gaps_of(lines, len)?,
    };
    for g in 0..ctx.gaps.len() {
        ctx.gaps[g].function = ctx.function_for(lines, g);
    }
    Some(ctx)
}

/// A line as the diff shows it: no `\r`, tabs four spaces wide.
fn display(raw: &str) -> String {
    raw.trim_end_matches('\r').replace('\t', "    ")
}

/// The gaps of a diff whose new side has `len` lines; `None` for a combined diff.
pub(super) fn gaps_of(lines: &[DiffLine], len: u32) -> Option<Vec<Gap>> {
    let mut gaps = Vec::new();
    // The last line of the previous hunk on each side.
    let (mut old_end, mut new_end) = (0u32, 0u32);
    for (row, line) in lines.iter().enumerate() {
        if line.kind != Kind::Hunk {
            continue;
        }
        let (os, on, ns, nn, _) = header_numbers(&line.raw)?;
        let (ob, nb) = (before(os, on), before(ns, nn));
        gaps.push(Gap {
            row,
            first: new_end + 1,
            last: nb,
            old_offset: i64::from(ob) - i64::from(nb),
            trailing: false,
            function: None,
        });
        (old_end, new_end) = (ob + on, nb + nn);
    }
    if gaps.is_empty() {
        return Some(gaps);
    }
    gaps.push(Gap {
        row: lines.len(),
        first: new_end + 1,
        last: len,
        old_offset: i64::from(old_end) - i64::from(new_end),
        trailing: true,
        function: None,
    });
    Some(gaps)
}

/// The text after a hunk header's ranges: the line git found the hunk's function on.
fn function_of(header: &str) -> Option<&str> {
    let (.., tail) = header_numbers(header)?;
    let name = tail.trim();
    (!name.is_empty()).then_some(name)
}

fn indent(line: &str) -> usize {
    display(line)
        .chars()
        .take_while(|c| c.is_whitespace())
        .count()
}

impl Context {
    /// The gap that folds on row `row`.
    pub fn gap_at(&self, row: usize) -> Option<usize> {
        self.gaps
            .iter()
            .position(|g| g.row == row && g.hidden() > 0)
    }

    /// The new side's lines `from..=to` as lines of context, numbered on the old side through
    /// `offset`, coloured as a run of their own; `None` past the file's end.
    fn lines(&self, from: u32, to: u32, offset: i64) -> Option<Vec<(DiffLine, Option<Spans>)>> {
        if from == 0 || to < from {
            return None;
        }
        let raw = self.raw.get(from as usize - 1..to as usize)?;
        let lines: Vec<DiffLine> = raw
            .iter()
            .zip(from..)
            .map(|(raw, n)| DiffLine {
                kind: Kind::Context,
                old: Some(old_of(n, offset)),
                new: Some(n),
                text: display(raw),
                raw: format!(" {raw}"),
            })
            .collect();
        let spans = (lines.len() <= MAX_COLOURED)
            .then(|| {
                highlight::highlight(&self.path, lines.iter().map(|l| l.text.as_str()), self.dark)
            })
            .flatten();
        let mut spans = spans.map(Vec::into_iter);
        Some(
            lines
                .into_iter()
                .map(|l| (l, spans.as_mut().and_then(Iterator::next)))
                .collect(),
        )
    }

    /// The function the hunk below gap `g` (its header at the gap's row of `lines`) is in, when
    /// it encloses the hunk.
    fn function_for(&self, lines: &[DiffLine], g: usize) -> Option<(u32, u32)> {
        let gap = self.gaps.get(g).filter(|g| !g.trailing)?;
        let header = lines.get(gap.row)?;
        let (_, _, ns, nn, _) = header_numbers(&header.raw)?;
        let span = self.function_span(&header.raw, gap.last + 1)?;
        // A line named that the hunk is past the end of (git names the last such line above).
        (span.1 > before(ns, nn)).then_some(span)
    }

    /// Whether _Show the enclosing function_ on gap `g` would open anything.
    pub fn function_opens(&self, g: usize) -> bool {
        let Some(gap) = self.gaps.get(g) else {
            return false;
        };
        let Some((start, end)) = gap.function else {
            return false;
        };
        let up = gap.hidden() > 0 && start <= gap.last;
        let down = self
            .gaps
            .get(g + 1)
            .is_some_and(|next| next.hidden() > 0 && end >= next.first);
        up || down
    }

    /// The first and last line of the function hunk `header` is in, as the new side numbers
    /// them: from the line git named in the header, up from `from`, to the first line after it
    /// as indented or less (a closing brace included).
    pub(super) fn function_span(&self, header: &str, from: u32) -> Option<(u32, u32)> {
        let name = function_of(header)?;
        let lowest = from.saturating_sub(MAX_FUNCTION_LINES).max(1);
        let start = (lowest..=from.min(self.raw.len() as u32))
            .rev()
            .find(|&n| display(&self.raw[n as usize - 1]).trim().starts_with(name))?;
        let depth = indent(&self.raw[start as usize - 1]);
        let mut end = start;
        for n in start + 1..=self.raw.len() as u32 {
            let line = &self.raw[n as usize - 1];
            if display(line).trim().is_empty() {
                continue;
            }
            end = n;
            if indent(line) <= depth {
                let closes = display(line).trim_start().starts_with(['}', ')', ']', 'e']);
                if !closes {
                    end = n - 1;
                }
                break;
            }
        }
        Some((start, end))
    }
}

/// What opening a gap changed in the rows: `count` rows put in (or taken out, when negative)
/// at row `at`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shift {
    pub at: usize,
    pub count: isize,
}

impl Shift {
    /// Where row `i` is now.
    pub fn row(self, i: usize) -> usize {
        if i < self.at {
            i
        } else {
            i.saturating_add_signed(self.count)
        }
    }
}

fn old_of(n: u32, offset: i64) -> u32 {
    u32::try_from(i64::from(n) + offset).unwrap_or(0)
}

/// The fold that hides the new side's line `line` (or the old side's, `old`), and how to open
/// it as far as that line and `around` lines past it, from the nearer hunk.
pub(super) fn unfold_to(
    ctx: &Context,
    line: u32,
    old: bool,
    around: u32,
) -> Option<(usize, Unfold)> {
    ctx.gaps.iter().enumerate().find_map(|(g, gap)| {
        let n = if old {
            u32::try_from(i64::from(line) - gap.old_offset).ok()?
        } else {
            line
        };
        if gap.hidden() == 0 || n < gap.first || n > gap.last {
            return None;
        }
        let up = gap.last - n + 1 + around;
        let down = n - gap.first + 1 + around;
        let how = if !gap.trailing && (up <= down || !gap.opens_down(g)) {
            Unfold::Up(up)
        } else {
            Unfold::Down(down)
        };
        Some((g, how))
    })
}

/// The row folding the unchanged lines after the last hunk.
pub(super) fn fold_row() -> DiffLine {
    DiffLine {
        kind: Kind::Fold,
        old: None,
        new: None,
        text: String::new(),
        raw: String::new(),
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;

mod mutation;

mod view_state;
