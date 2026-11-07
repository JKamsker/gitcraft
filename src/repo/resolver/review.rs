//! What the agent did to a file, to review before applying it: each conflict with its two
//! sides and what the agent wrote in its place, and any line it changed outside them; or, when
//! its result can't be lined up with the conflicts, the file before and after as a line diff.

use crate::repo::conflict::{Segment, parse};

/// How the agent's text for a conflict relates to its sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Took {
    Ours,
    Theirs,
    /// Ours, then theirs.
    Both,
    /// Theirs, then ours.
    BothSwapped,
    /// Nothing of either.
    Removed,
    /// Text of its own: a mix, or a rewrite.
    Rewrote,
}

impl Took {
    fn of(ours: &str, theirs: &str, result: &str) -> Took {
        if result == ours {
            Took::Ours
        } else if result == theirs {
            Took::Theirs
        } else if result == format!("{ours}{theirs}") {
            Took::Both
        } else if result == format!("{theirs}{ours}") {
            Took::BothSwapped
        } else if result.trim().is_empty() {
            Took::Removed
        } else {
            Took::Rewrote
        }
    }

    /// How it reads: "took main", "kept both"…
    pub(super) fn words(self, ours: &str, theirs: &str) -> String {
        match self {
            Took::Ours => format!("took {ours}"),
            Took::Theirs => format!("took {theirs}"),
            Took::Both => format!("kept both, {ours} first"),
            Took::BothSwapped => format!("kept both, {theirs} first"),
            Took::Removed => "removed both".to_owned(),
            Took::Rewrote => "combined in its own words".to_owned(),
        }
    }
}

/// One conflict and how the agent resolved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Resolved {
    pub ours: String,
    pub base: Option<String>,
    pub theirs: String,
    /// What the agent wrote in its place.
    pub result: String,
    /// The line it starts at in the agent's version, from 1.
    pub line: usize,
    pub took: Took,
}

/// A line of a line diff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Line {
    Same(String),
    Removed(String),
    Added(String),
    /// Unchanged lines left out between changes, how many.
    Skipped(usize),
}

/// What the agent did to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FileReview {
    /// Its conflicts, each with the agent's text, and whether it changed anything else.
    Conflicts {
        resolved: Vec<Resolved>,
        ours_label: String,
        theirs_label: String,
        changed_outside: bool,
        diff: Vec<Line>,
    },
    /// No conflict to line up (none in it, or the agent's text doesn't follow the file's):
    /// before and after.
    Diff(Vec<Line>),
    /// The agent removed the file.
    Deleted { before: String },
    /// It made the file, which wasn't in the checkout.
    Created { after: String },
    /// Not text: what it is shows once applied, in gitcraft's own diff.
    Binary,
    /// It couldn't be read (a link, not a file): it isn't applied either.
    Unreadable(String),
}

/// What the agent did to `path` of `ws`.
pub(super) fn of_file(ws: &crate::resolver::workspace::Workspace, path: &str) -> FileReview {
    let (before, after) = match ws.before_and_after(path) {
        Ok(both) => both,
        Err(e) => return FileReview::Unreadable(e),
    };
    let text = |b: &[u8]| std::str::from_utf8(b).map(str::to_owned);
    let before = before.map(text).transpose();
    let after = after.as_deref().map(text).transpose();
    match (before, after) {
        (Ok(b), Ok(a)) => review(b.as_deref(), a.as_deref()),
        _ => FileReview::Binary,
    }
}

/// The most lines of a side compared line by line; past that, the changed part is shown
/// whole, removed then added.
const MAX_CELLS: usize = 4_000_000;
/// Unchanged lines kept around each change.
const CONTEXT: usize = 3;

/// What became of `before` (the file with conflict markers) in `after` (the agent's).
pub(super) fn review(before: Option<&str>, after: Option<&str>) -> FileReview {
    let (before, after) = match (before, after) {
        (Some(b), Some(a)) => (b, a),
        (Some(b), None) => {
            return FileReview::Deleted {
                before: b.to_owned(),
            };
        }
        (None, Some(a)) => {
            return FileReview::Created {
                after: a.to_owned(),
            };
        }
        (None, None) => return FileReview::Diff(Vec::new()),
    };
    let a: Vec<&str> = before.split_inclusive('\n').collect();
    let b: Vec<&str> = after.split_inclusive('\n').collect();
    let ops = ops(&a, &b);
    let diff = with_context(lines(&a, &b, &ops));
    let (segments, ours_label, theirs_label) = parse(before);
    if !segments
        .iter()
        .any(|s| matches!(s, Segment::Conflict { .. }))
    {
        return FileReview::Diff(diff);
    }
    let aligned = align(&segments, after).or_else(|| align_by_lines(&segments, &b, &ops));
    match aligned {
        Some((resolved, changed_outside)) => FileReview::Conflicts {
            resolved,
            ours_label: label(&ours_label, "ours"),
            theirs_label: label(&theirs_label, "theirs"),
            changed_outside,
            diff,
        },
        None => FileReview::Diff(diff),
    }
}

fn label(marker: &str, fallback: &str) -> String {
    if marker.is_empty() {
        fallback.to_owned()
    } else {
        marker.to_owned()
    }
}

fn resolved(seg: (&str, Option<&String>, &str), result: String, line: usize) -> Resolved {
    let (ours, base, theirs) = seg;
    Resolved {
        took: Took::of(ours, theirs, &result),
        line,
        ours: ours.to_owned(),
        base: base.cloned(),
        theirs: theirs.to_owned(),
        result,
    }
}

/// Lines the conflicts up with `after` when the agent left the rest of the file as it was:
/// the file's common parts, found in order, and between them what it wrote for each conflict.
/// `None` when a common part isn't there as it was.
fn align(segments: &[Segment], after: &str) -> Option<(Vec<Resolved>, bool)> {
    let mut at = 0usize;
    let mut out = Vec::new();
    let mut open: Option<(&str, Option<&String>, &str)> = None;
    let last = segments.len().saturating_sub(1);
    for (i, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Common(common) => {
                let start = match open {
                    // Before any conflict, or right after another common part: here.
                    None => after[at..].starts_with(common.as_str()).then_some(at)?,
                    // The last part ends the file; one between conflicts is the next match.
                    Some(_) if i == last => {
                        let start = after.len().checked_sub(common.len())?;
                        (start >= at && after[start..] == **common).then_some(start)?
                    }
                    Some(_) => at + after[at..].find(common.as_str())?,
                };
                if let Some(seg) = open.take() {
                    let line = after[..at].matches('\n').count() + 1;
                    out.push(resolved(seg, after[at..start].to_owned(), line));
                }
                at = start + common.len();
            }
            Segment::Conflict { ours, base, theirs } => {
                if let Some(seg) = open.take() {
                    // Two conflicts with nothing between them: the first got nothing.
                    let line = after[..at].matches('\n').count() + 1;
                    out.push(resolved(seg, String::new(), line));
                }
                open = Some((ours, base.as_ref(), theirs));
            }
        }
    }
    match open {
        Some(seg) => {
            let line = after[..at].matches('\n').count() + 1;
            out.push(resolved(seg, after[at..].to_owned(), line));
        }
        None if at != after.len() => return None,
        None => {}
    }
    Some((out, false))
}

/// Lines the conflicts up through the line diff, when the agent changed the file around them
/// too: what it wrote for a conflict is what lies between the lines kept on either side of
/// it. `changed_outside` is then whether a line outside every conflict was changed.
fn align_by_lines(segments: &[Segment], b: &[&str], ops: &[Op]) -> Option<(Vec<Resolved>, bool)> {
    // Each conflict's lines in the file with markers, from its `<<<<<<<` to its `>>>>>>>`.
    let count = |s: &str| s.split_inclusive('\n').count();
    let mut ranges = Vec::new();
    let mut line = 0;
    for segment in segments {
        match segment {
            Segment::Common(text) => line += count(text),
            Segment::Conflict { ours, base, theirs } => {
                let base_lines = base.as_ref().map_or(0, |b| 1 + count(b));
                let n = 3 + count(ours) + base_lines + count(theirs);
                ranges.push((
                    line,
                    line + n,
                    (ours.as_str(), base.as_ref(), theirs.as_str()),
                ));
                line += n;
            }
        }
    }
    let kept: Vec<(usize, usize)> = ops
        .iter()
        .filter_map(|op| match op {
            Op::Same(i, j) => Some((*i, *j)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut taken = vec![false; b.len()];
    for (s, e, seg) in ranges {
        let from = kept
            .iter()
            .rev()
            .find(|(i, _)| *i < s)
            .map_or(0, |(_, j)| j + 1);
        let to = kept
            .iter()
            .find(|(i, _)| *i >= e)
            .map_or(b.len(), |(_, j)| *j);
        // Two conflicts sharing lines: nothing tells which wrote what.
        if to < from || taken[from..to].iter().any(|&t| t) {
            return None;
        }
        taken[from..to].iter_mut().for_each(|t| *t = true);
        out.push(resolved(seg, b[from..to].concat(), from + 1));
    }
    if out.is_empty() {
        return None;
    }
    let changed_outside = ops.iter().any(|op| match op {
        Op::Add(j) => !taken[*j],
        Op::Del(i) => !in_conflict(segments, *i),
        Op::Same(..) => false,
    });
    Some((out, changed_outside))
}

/// Whether line `i` of the file with markers is part of a conflict.
fn in_conflict(segments: &[Segment], i: usize) -> bool {
    let count = |s: &str| s.split_inclusive('\n').count();
    let mut line = 0;
    for segment in segments {
        let n = match segment {
            Segment::Common(text) => count(text),
            Segment::Conflict { ours, base, theirs } => {
                3 + count(ours) + base.as_ref().map_or(0, |b| 1 + count(b)) + count(theirs)
            }
        };
        if i < line + n {
            return matches!(segment, Segment::Conflict { .. });
        }
        line += n;
    }
    false
}

/// A step of a line diff: a line both keep (its index in each), one removed, one added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Same(usize, usize),
    Del(usize),
    Add(usize),
}

/// How `a` becomes `b`, line by line: by the longest run of lines both keep, after the lines
/// they start and end with alike (removed then added past [`MAX_CELLS`]).
fn ops(a: &[&str], b: &[&str]) -> Vec<Op> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (an, bn) = (a.len() - suffix, b.len() - suffix);
    let mut out: Vec<Op> = (0..prefix).map(|i| Op::Same(i, i)).collect();
    let (am, bm) = (&a[prefix..an], &b[prefix..bn]);
    if am.len().saturating_mul(bm.len()) > MAX_CELLS {
        out.extend((prefix..an).map(Op::Del));
        out.extend((prefix..bn).map(Op::Add));
    } else {
        // `kept[i][j]`: lines kept by am[i..] and bm[j..].
        let w = bm.len() + 1;
        let mut kept = vec![0u32; (am.len() + 1) * w];
        for i in (0..am.len()).rev() {
            for j in (0..bm.len()).rev() {
                kept[i * w + j] = if am[i] == bm[j] {
                    kept[(i + 1) * w + j + 1] + 1
                } else {
                    kept[(i + 1) * w + j].max(kept[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < am.len() || j < bm.len() {
            if i < am.len() && j < bm.len() && am[i] == bm[j] {
                out.push(Op::Same(prefix + i, prefix + j));
                i += 1;
                j += 1;
            } else if i < am.len()
                && (j == bm.len() || kept[(i + 1) * w + j] >= kept[i * w + j + 1])
            {
                // Removed lines come before the lines added in their place.
                out.push(Op::Del(prefix + i));
                i += 1;
            } else {
                out.push(Op::Add(prefix + j));
                j += 1;
            }
        }
    }
    out.extend((0..suffix).map(|k| Op::Same(an + k, bn + k)));
    out
}

fn bare(line: &str) -> String {
    line.trim_end_matches(['\n', '\r']).to_owned()
}

fn lines(a: &[&str], b: &[&str], ops: &[Op]) -> Vec<Line> {
    ops.iter()
        .map(|op| match *op {
            Op::Same(i, _) => Line::Same(bare(a[i])),
            Op::Del(i) => Line::Removed(bare(a[i])),
            Op::Add(j) => Line::Added(bare(b[j])),
        })
        .collect()
}

/// `before` and `after` line by line, unchanged runs cut to a few lines around the changes.
#[cfg(test)]
fn line_diff(before: &str, after: &str) -> Vec<Line> {
    let a: Vec<&str> = before.split_inclusive('\n').collect();
    let b: Vec<&str> = after.split_inclusive('\n').collect();
    with_context(lines(&a, &b, &ops(&a, &b)))
}

/// Unchanged runs longer than the context around changes become one [`Line::Skipped`].
fn with_context(lines: Vec<Line>) -> Vec<Line> {
    let changed: Vec<bool> = lines.iter().map(|l| !matches!(l, Line::Same(_))).collect();
    let near = |i: usize| {
        let from = i.saturating_sub(CONTEXT);
        let to = (i + CONTEXT + 1).min(changed.len());
        changed[from..to].iter().any(|&c| c)
    };
    let mut out = Vec::new();
    let mut skipped = 0;
    for (i, line) in lines.into_iter().enumerate() {
        if near(i) {
            if skipped > 0 {
                out.push(Line::Skipped(skipped));
                skipped = 0;
            }
            out.push(line);
        } else {
            skipped += 1;
        }
    }
    if skipped > 0 {
        out.push(Line::Skipped(skipped));
    }
    out
}

#[cfg(test)]
mod tests;
