//! Moved code: a block of lines removed in one place of the file and added in another. Each
//! block gets a muted colour of its own and a marker; _Moved to…_ shows its two places together
//! with what was edited on the way.
//!
//! Lines are compared without the whitespace around them, so a block moved into (or out of) a
//! deeper indentation still counts, as git's `--color-moved-ws=allow-indentation-change` would.

use std::collections::HashMap;
use std::ops::Range;

use super::{DiffLine, Kind};

/// Fewer letters and digits than this in a block and it is not a move: closing braces, blank
/// lines and `else` turn up everywhere (git's `--color-moved` uses the same threshold).
pub(super) const MIN_ALNUM: usize = 20;
/// Lines of one place that may sit between two moved pieces of a block and still make it one
/// block, edited during the move.
const MAX_EDIT_GAP: usize = 3;
/// A line that occurs this often among the additions starts no block: lines like `}` or `end`
/// would make looking for moves quadratic.
const MAX_CANDIDATES: usize = 64;
/// Past this many pairs of lines to compare, a block's comparison is shown removed then added.
const MAX_COMPARE_CELLS: usize = 1_000_000;

/// A block of removed lines and the block of added lines it became, as indices into the
/// diff's lines (the last index excluded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovedBlock {
    pub del: Range<usize>,
    pub add: Range<usize>,
    /// No line changed on the way (whitespace around lines aside).
    pub exact: bool,
}

/// The moved blocks of a diff, and for each of its lines the block it is in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moves {
    pub blocks: Vec<MovedBlock>,
    /// Per line: its block, and whether the line is also on the block's other side (`false`:
    /// edited during the move).
    pub of_line: Vec<Option<(usize, bool)>>,
}

impl Moves {
    /// The block line `i` is in, and whether it is the same in the other place.
    pub fn at(&self, i: usize) -> Option<(usize, bool)> {
        self.of_line.get(i).copied().flatten()
    }
}

/// The text a line is compared by: without the whitespace around it.
fn norm(line: &DiffLine) -> &str {
    // The line as git has it, not as shown (tabs expanded, a very long one cut).
    line.raw.get(1..).unwrap_or_default().trim()
}

fn alnum(s: &str) -> usize {
    s.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Numbers each run of changed lines (removals and the additions after them, up to the next
/// line of context or hunk): lines of the same change were edited in place, not moved.
fn change_ids(lines: &[DiffLine]) -> Vec<usize> {
    let mut id = 0;
    let mut in_change = false;
    lines
        .iter()
        .map(|l| {
            match l.kind {
                Kind::Add | Kind::Del if !in_change => {
                    id += 1;
                    in_change = true;
                }
                // A "\ No newline at end of file" note doesn't end the change.
                Kind::Add | Kind::Del | Kind::Meta => {}
                Kind::Context | Kind::Hunk => in_change = false,
            }
            id
        })
        .collect()
}

/// Finds the blocks of `lines` (one file's diff) removed in one place and added in another.
pub fn detect(lines: &[DiffLine]) -> Moves {
    let change = change_ids(lines);
    let mut adds: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if l.kind == Kind::Add && alnum(norm(l)) > 0 {
            adds.entry(norm(l)).or_default().push(i);
        }
    }
    let mut used = vec![false; lines.len()];
    let mut pieces: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    let mut d = 0;
    while d < lines.len() {
        let best = (lines[d].kind == Kind::Del && !used[d])
            .then(|| best_match(lines, &change, &used, &adds, d))
            .flatten();
        match best {
            Some(len_a) => {
                let (len, a) = len_a;
                for k in 0..len {
                    used[d + k] = true;
                    used[a + k] = true;
                }
                pieces.push((d..d + len, a..a + len));
                d += len;
            }
            None => d += 1,
        }
    }
    // Lines kept between the edits of a block moved and edited, too short to be moves on
    // their own (`}`, `return x;`), go with the piece they follow.
    pieces.sort_by_key(|(del, _)| del.start);
    let mut followers = Vec::new();
    for piece in &pieces {
        let mut at = piece.clone();
        while let Some(next) = follow(lines, &change, &used, &at) {
            for i in next.0.clone().chain(next.1.clone()) {
                used[i] = true;
            }
            followers.push(next.clone());
            at = next;
        }
    }
    pieces.extend(followers);
    let blocks = merge_pieces(lines, &used, pieces);
    let mut of_line = vec![None; lines.len()];
    for (b, block) in blocks.iter().enumerate() {
        for i in block.del.clone().chain(block.add.clone()) {
            of_line[i] = Some((b, used[i]));
        }
    }
    Moves { blocks, of_line }
}

/// The longest run of removed lines from `d` that is also a run of added lines in another
/// change, with enough letters to be a move: its length and where the added run starts.
fn best_match(
    lines: &[DiffLine],
    change: &[usize],
    used: &[bool],
    adds: &HashMap<&str, Vec<usize>>,
    d: usize,
) -> Option<(usize, usize)> {
    let starts = adds.get(norm(&lines[d]))?;
    if starts.len() > MAX_CANDIDATES {
        return None;
    }
    let mut best: Option<(usize, usize, usize)> = None;
    for &a in starts {
        if used[a] || change[a] == change[d] {
            continue;
        }
        let mut len = 0;
        let mut letters = 0;
        while let (Some(x), Some(y)) = (lines.get(d + len), lines.get(a + len)) {
            let fits = x.kind == Kind::Del
                && y.kind == Kind::Add
                && !used[d + len]
                && !used[a + len]
                && change[a + len] != change[d + len]
                && norm(x) == norm(y);
            if !fits {
                break;
            }
            letters += alnum(norm(x));
            len += 1;
        }
        if letters >= MIN_ALNUM && best.is_none_or(|(_, _, l)| letters > l) {
            best = Some((len, a, letters));
        }
    }
    best.map(|(len, a, _)| (len, a))
}

/// The next piece of the block that `at` is part of: lines the same in both places, a few
/// edited lines after `at` at most, however few letters they have.
fn follow(
    lines: &[DiffLine],
    change: &[usize],
    used: &[bool],
    (del, add): &(Range<usize>, Range<usize>),
) -> Option<(Range<usize>, Range<usize>)> {
    let free = |i: usize, kind: Kind| lines.get(i).is_some_and(|l| l.kind == kind) && !used[i];
    for gap in 0..=MAX_EDIT_GAP * 2 {
        for skip_d in 0..=gap.min(MAX_EDIT_GAP) {
            let skip_a = gap - skip_d;
            if skip_a > MAX_EDIT_GAP {
                continue;
            }
            let (d, a) = (del.end + skip_d, add.end + skip_a);
            let between = |from: usize, to: usize, kind: Kind| {
                (from..to).all(|i| lines.get(i).is_some_and(|l| l.kind == kind) && !used[i])
            };
            if !between(del.end, d, Kind::Del) || !between(add.end, a, Kind::Add) {
                continue;
            }
            let mut len = 0;
            while free(d + len, Kind::Del)
                && free(a + len, Kind::Add)
                && change[d + len] != change[a + len]
                && norm(&lines[d + len]) == norm(&lines[a + len])
            {
                len += 1;
            }
            if len > 0 {
                return Some((d..d + len, a..a + len));
            }
        }
    }
    None
}

/// Joins pieces moved together, with a few lines edited between them, into one block.
fn merge_pieces(
    lines: &[DiffLine],
    used: &[bool],
    mut pieces: Vec<(Range<usize>, Range<usize>)>,
) -> Vec<MovedBlock> {
    pieces.sort_by_key(|(del, _)| del.start);
    let mut blocks: Vec<MovedBlock> = Vec::new();
    for (del, add) in pieces {
        if let Some(last) = blocks.last_mut()
            && joins(lines, used, &last.del, &del, Kind::Del)
            && joins(lines, used, &last.add, &add, Kind::Add)
        {
            let edited = last.del.end != del.start || last.add.end != add.start;
            last.exact &= !edited;
            last.del.end = del.end;
            last.add.end = add.end;
            continue;
        }
        blocks.push(MovedBlock {
            del,
            add,
            exact: true,
        });
    }
    blocks
}

/// Whether `next` follows `prev` closely enough, with only lines of `kind` between them, to be
/// the same block.
fn joins(
    lines: &[DiffLine],
    used: &[bool],
    prev: &Range<usize>,
    next: &Range<usize>,
    kind: Kind,
) -> bool {
    // Lines of another block between them would end up in two blocks.
    next.start >= prev.end
        && next.start - prev.end <= MAX_EDIT_GAP
        && (prev.end..next.start).all(|i| lines[i].kind == kind && !used[i])
}

/// One row of a moved block's comparison: the line where it was, the line where it is now.
pub type CompareRow = (Option<usize>, Option<usize>);

/// The block's removed lines against its added ones, in order: pairs of the same line, and the
/// lines found only on one side (edited during the move).
pub fn compare(lines: &[DiffLine], block: &MovedBlock) -> Vec<CompareRow> {
    let old: Vec<usize> = block.del.clone().collect();
    let new: Vec<usize> = block.add.clone().collect();
    let (n, m) = (old.len(), new.len());
    if block.exact && n == m {
        return old
            .into_iter()
            .zip(new)
            .map(|(i, j)| (Some(i), Some(j)))
            .collect();
    }
    if n.saturating_mul(m) > MAX_COMPARE_CELLS {
        let mut out: Vec<CompareRow> = old.iter().map(|&i| (Some(i), None)).collect();
        out.extend(new.iter().map(|&j| (None, Some(j))));
        return out;
    }
    let same = |i: usize, j: usize| norm(&lines[old[i]]) == norm(&lines[new[j]]);
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if same(i, j) {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::with_capacity(n.max(m));
    while i < n || j < m {
        if i < n && j < m && same(i, j) {
            out.push((Some(old[i]), Some(new[j])));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push((Some(old[i]), None));
            i += 1;
        } else {
            out.push((None, Some(new[j])));
            j += 1;
        }
    }
    out
}
