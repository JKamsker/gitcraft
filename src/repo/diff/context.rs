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
    spans: Option<Vec<Spans>>,
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
    let rev = new_side(key)?;
    let path = key.path();
    let bytes = git::file_at(repo, rev.as_deref(), path).ok()?;
    if bytes.len() > MAX_FILE_BYTES {
        return None;
    }
    // Not UTF-8: lines read with their odd bytes replaced would be wrong context to stage with.
    let text = String::from_utf8(bytes).ok()?;
    let raw: Vec<String> = text.split_terminator('\n').map(str::to_owned).collect();
    let shown: Vec<String> = raw.iter().map(|l| display(l)).collect();
    let gaps = gaps_of(lines, raw.len() as u32)?;
    let spans = highlight::highlight(path, shown.iter().map(String::as_str), dark);
    Some(Context {
        raw,
        spans,
        no_newline_at_end: !text.is_empty() && !text.ends_with('\n'),
        gaps,
    })
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

    /// The new side's line `n` (from 1) as a line of context at old line `old`, with its
    /// colours.
    fn line(&self, n: u32, old: u32) -> Option<(DiffLine, Option<Spans>)> {
        let raw = self.raw.get(n as usize - 1)?;
        let spans = self
            .spans
            .as_ref()
            .and_then(|s| s.get(n as usize - 1).cloned());
        Some((
            DiffLine {
                kind: Kind::Context,
                old: Some(old),
                new: Some(n),
                text: display(raw),
                raw: format!(" {raw}"),
            },
            spans,
        ))
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

impl DiffData {
    /// Opens gap `g` as `how` says; returns how the rows moved, in order.
    pub fn unfold(&mut self, g: usize, how: Unfold) -> Vec<Shift> {
        let Some(gap) = self.context.as_ref().and_then(|c| c.gaps.get(g)).cloned() else {
            return Vec::new();
        };
        let hidden = gap.hidden();
        match how {
            Unfold::Up(n) => self.open_up(g, n.min(hidden)).into_iter().collect(),
            Unfold::Down(n) => self.open_down(g, n.min(hidden)),
            Unfold::All if gap.opens_down(g) => self.open_down(g, hidden),
            Unfold::All => self.open_up(g, hidden).into_iter().collect(),
            Unfold::Function => self.open_function(g),
        }
    }

    /// Shows the `n` lines right above the hunk below gap `g`: they become its first lines.
    fn open_up(&mut self, g: usize, n: u32) -> Option<Shift> {
        let ctx = self.context.as_mut()?;
        let gap = ctx.gaps.get(g)?.clone();
        if n == 0 || gap.trailing {
            return None;
        }
        let header = self.lines.get(gap.row)?;
        let (os, on, ns, nn, tail) = header_numbers(&header.raw)?;
        let from = gap.last + 1 - n;
        let new: Vec<_> = (from..=gap.last)
            .filter_map(|k| ctx.line(k, old_of(k, gap.old_offset)))
            .collect();
        if new.len() != n as usize {
            return None;
        }
        let first_old = old_of(from, gap.old_offset);
        let raw = format!("@@ -{first_old},{} +{from},{} @@{tail}", on + n, nn + n);
        // An empty range starts at the line before it; one with lines at its first line.
        let _ = (os, ns);
        ctx.gaps[g].last -= n;
        self.set_header(gap.row, raw);
        Some(self.insert(gap.row + 1, new))
    }

    /// Shows the `n` lines right below the hunk above gap `g`: they become its last lines (and
    /// past the end of the file, the note that its last line has no newline).
    fn open_down(&mut self, g: usize, n: u32) -> Vec<Shift> {
        let Some(ctx) = self.context.as_mut() else {
            return Vec::new();
        };
        let Some(gap) = ctx.gaps.get(g).cloned() else {
            return Vec::new();
        };
        if n == 0 || !gap.opens_down(g) {
            return Vec::new();
        }
        let to = gap.first + n - 1;
        let mut new: Vec<_> = (gap.first..=to)
            .filter_map(|k| ctx.line(k, old_of(k, gap.old_offset)))
            .collect();
        if new.len() != n as usize {
            return Vec::new();
        }
        if gap.trailing && to as usize == ctx.raw.len() && ctx.no_newline_at_end {
            new.push((super::parse::meta("\\ No newline at end of file"), None));
        }
        ctx.gaps[g].first += n;
        let emptied = ctx.gaps[g].hidden() == 0;
        // The hunk above gets `n` more lines on both sides.
        if let Some(h) = self.lines[..gap.row]
            .iter()
            .rposition(|l| l.kind == Kind::Hunk)
            && let Some((os, on, ns, nn, tail)) = header_numbers(&self.lines[h].raw)
        {
            let raw = format!("@@ -{os},{} +{ns},{} @@{tail}", on + n, nn + n);
            self.set_header(h, raw);
        }
        let mut shifts = vec![self.insert(gap.row, new)];
        if gap.trailing && emptied {
            let row = shifts[0].row(gap.row);
            shifts.extend(self.remove_fold_row(row));
        }
        shifts
    }

    /// Opens gap `g` and the one after it as far as the function of the hunk between them.
    fn open_function(&mut self, g: usize) -> Vec<Shift> {
        let Some(ctx) = self.context.as_ref() else {
            return Vec::new();
        };
        let Some(gap) = ctx.gaps.get(g) else {
            return Vec::new();
        };
        let Some(header) = self.lines.get(gap.row) else {
            return Vec::new();
        };
        let Some((_, _, ns, nn, _)) = header_numbers(&header.raw) else {
            return Vec::new();
        };
        let Some((start, end)) = ctx.function_span(&header.raw, gap.last + 1) else {
            return Vec::new();
        };
        let up = if start < gap.first {
            gap.hidden()
        } else {
            (gap.last + 1).saturating_sub(start).min(gap.hidden())
        };
        let hunk_end = before(ns, nn) + nn;
        let down = ctx
            .gaps
            .get(g + 1)
            .map_or(0, |next| end.saturating_sub(hunk_end).min(next.hidden()));
        let mut shifts: Vec<Shift> = self.open_up(g, up).into_iter().collect();
        shifts.extend(self.open_down(g + 1, down));
        shifts
    }

    /// Puts `new` lines in at row `at`, moving everything after them down.
    fn insert(&mut self, at: usize, new: Vec<(DiffLine, Option<Spans>)>) -> Shift {
        let shift = Shift {
            at,
            count: new.len() as isize,
        };
        let mut lines = Vec::with_capacity(new.len());
        let mut spans = Vec::with_capacity(new.len());
        for (mut line, mut span) in new {
            if let Some(whole) = highlight::cap_line(&mut line.text, span.as_mut()) {
                self.uncut.insert(usize::MAX - lines.len(), whole);
            }
            self.longest = self.longest.max(line.text.chars().count());
            lines.push(line);
            spans.push(span);
        }
        let count = lines.len();
        self.shift_rows(shift);
        // The cut lines were noted at stand-in rows; now that the rows moved, at their own.
        for k in 0..count {
            if let Some(whole) = self.uncut.remove(&(usize::MAX - k)) {
                self.uncut.insert(at + k, whole);
            }
        }
        let pos = self
            .split
            .iter()
            .position(|&(l, r)| l.max(r).is_some_and(|i| i >= at + count))
            .unwrap_or(self.split.len());
        self.split
            .splice(pos..pos, (at..at + count).map(|i| (Some(i), Some(i))));
        self.lines.splice(at..at, lines);
        self.spans.splice(at..at, spans);
        self.changed
            .splice(at..at, std::iter::repeat_n(None, count));
        self.moves
            .of_line
            .splice(at..at, std::iter::repeat_n(None, count));
        shift
    }

    /// Takes out the trailing fold row at `row`, once it hides nothing.
    fn remove_fold_row(&mut self, row: usize) -> Option<Shift> {
        if self.lines.get(row)?.kind != Kind::Fold {
            return None;
        }
        self.lines.remove(row);
        self.spans.remove(row);
        self.changed.remove(row);
        if row < self.moves.of_line.len() {
            self.moves.of_line.remove(row);
        }
        self.split
            .retain(|&(l, r)| l != Some(row) && r != Some(row));
        let shift = Shift {
            at: row + 1,
            count: -1,
        };
        self.shift_rows(shift);
        Some(shift)
    }

    /// Renumbers what points at rows from `shift.at` on.
    fn shift_rows(&mut self, shift: Shift) {
        let row = |i: usize| shift.row(i);
        for (l, r) in &mut self.split {
            *l = l.map(row);
            *r = r.map(row);
        }
        let uncut = std::mem::take(&mut self.uncut);
        self.uncut = uncut
            .into_iter()
            .map(|(i, s)| {
                if i > usize::MAX / 2 {
                    (i, s)
                } else {
                    (row(i), s)
                }
            })
            .collect();
        for block in &mut self.moves.blocks {
            block.del = row(block.del.start)..row(block.del.end);
            block.add = row(block.add.start)..row(block.add.end);
        }
        if let Some(ctx) = &mut self.context {
            for gap in &mut ctx.gaps {
                gap.row = row(gap.row);
            }
        }
        self.map = Default::default();
    }

    /// Writes a hunk header anew, as shown and as patches read it.
    fn set_header(&mut self, row: usize, raw: String) {
        if let Some(line) = self.lines.get_mut(row) {
            line.text = raw.trim_end_matches('\r').to_owned();
            line.raw = raw;
        }
    }
}

/// Line `n` of the new side as the old side numbers it, in a gap with `offset`.
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

impl crate::repo::DiffView {
    /// Opens gap `g` of the diff shown as `how` says, keeping what points at its rows (lines
    /// picked, text selected, the line to scroll to) on the same lines; the lines opened stay
    /// open when the diff is read again.
    pub(in crate::repo) fn unfold(&mut self, g: usize, how: Unfold) {
        let Some(Ok(data)) = &mut self.data else {
            return;
        };
        let shifts = data.unfold(g, how);
        let mut opened = Vec::new();
        for shift in &shifts {
            if shift.count > 0 {
                let added = shift.at..shift.at + shift.count as usize;
                opened.extend(data.lines[added].iter().filter_map(|l| l.new));
            }
        }
        self.revealed.extend(opened);
        for shift in shifts {
            self.follow_rows(shift);
        }
        self.recompare_moved();
    }

    /// Opens again the lines opened before the diff was read again.
    pub(in crate::repo) fn reopen(&mut self) {
        let Some(Ok(data)) = &mut self.data else {
            return;
        };
        let Some(ctx) = &data.context else { return };
        let mut wanted = Vec::new();
        for (g, gap) in ctx.gaps.iter().enumerate() {
            let below = (gap.first..=gap.last)
                .rev()
                .take_while(|n| self.revealed.contains(n))
                .count() as u32;
            let above = (gap.first..=gap.last.saturating_sub(below))
                .take_while(|n| self.revealed.contains(n))
                .count() as u32;
            if !gap.trailing {
                wanted.push((g, Unfold::Up(below)));
            }
            if gap.opens_down(g) {
                let above = if gap.trailing {
                    above.max(below)
                } else {
                    above
                };
                wanted.push((g, Unfold::Down(above)));
            }
        }
        let shifts: Vec<Shift> = wanted
            .into_iter()
            .flat_map(|(g, how)| data.unfold(g, how))
            .collect();
        for shift in shifts {
            self.follow_rows(shift);
        }
    }

    /// The moved block shown, compared again on the rows as they are now.
    fn recompare_moved(&mut self) {
        if let (Some(Ok(data)), Some(open)) = (&self.data, &self.moved) {
            self.moved = super::moved_view::Opened::new(data, open.block);
        }
    }

    /// Moves what points at rows along with `shift`.
    fn follow_rows(&mut self, shift: Shift) {
        let row = |i: usize| shift.row(i);
        self.chosen = self.chosen.iter().map(|&i| row(i)).collect();
        self.anchor = self.anchor.map(row);
        self.jump_line = self.jump_line.map(row);
        self.text_sel = self.text_sel.map(|mut s| {
            s.anchor.0 = row(s.anchor.0);
            s.cursor.0 = row(s.cursor.0);
            s
        });
        // A change is jumped to by its place among the changes, which doesn't move; where the
        // view was left does.
        self.at_change = None;
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
