//! The order Enter steps through the matches in: the order of the rows shown, and in a row of
//! a side-by-side diff the old half before the new one.

use std::ops::Range;

/// One match, where it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Hit {
    /// The row it is on (in a side-by-side diff, rows hold two lines).
    pub(crate) row: usize,
    /// 0: the row's line or its left half, 1: its right half.
    pub(crate) side: u8,
    /// Its characters in the line.
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// The line it is in.
    pub(crate) line: usize,
}

impl Hit {
    pub(crate) fn chars(&self) -> Range<usize> {
        self.start..self.end
    }
}

/// How the lines are laid out in rows.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Layout<'a> {
    /// A line per row.
    Lines,
    /// Side by side: each row's (left line, right line). A line on both sides (a diff's
    /// unchanged one) counts once, on the left.
    Split(&'a [(Option<usize>, Option<usize>)]),
}

impl Layout<'_> {
    pub(crate) fn is_split(&self) -> bool {
        matches!(self, Layout::Split(_))
    }
}

/// Every match of `found` (lines in order, with their matched characters), in the order of the
/// rows of `layout`.
pub(crate) fn ordered(found: &[(usize, Vec<Range<usize>>)], layout: Layout<'_>) -> Vec<Hit> {
    let hit = |row: usize, side: u8, line: usize, r: &Range<usize>| Hit {
        row,
        side,
        start: r.start,
        end: r.end,
        line,
    };
    match layout {
        Layout::Lines => found
            .iter()
            .flat_map(|(line, ranges)| ranges.iter().map(|r| hit(*line, 0, *line, r)))
            .collect(),
        Layout::Split(rows) => {
            let places = places_of(rows, found.last().map_or(0, |(l, _)| l + 1));
            let mut hits: Vec<Hit> = found
                .iter()
                .filter_map(|(line, ranges)| {
                    let (row, side) = places.get(*line).copied().flatten()?;
                    Some(ranges.iter().map(move |r| hit(row, side, *line, r)))
                })
                .flatten()
                .collect();
            hits.sort_unstable();
            hits
        }
    }
}

/// The row and the side each of the first `lines` lines shows on.
fn places_of(rows: &[(Option<usize>, Option<usize>)], lines: usize) -> Vec<Option<(usize, u8)>> {
    let mut places = vec![None; lines];
    for (row, &(left, right)) in rows.iter().enumerate() {
        for (side, line) in [(0u8, left), (1, right.filter(|r| Some(*r) != left))] {
            if let Some(slot) = line.and_then(|l| places.get_mut(l)) {
                *slot = Some((row, side));
            }
        }
    }
    places
}

#[cfg(test)]
// Expected matches are lists of ranges, some of one.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    #[test]
    fn a_line_per_row_keeps_the_lines_order() {
        let found = vec![(2, vec![0..1, 4..5]), (7, vec![3..4])];
        let hits = ordered(&found, Layout::Lines);
        let at: Vec<(usize, usize)> = hits.iter().map(|h| (h.row, h.start)).collect();
        assert_eq!(at, [(2, 0), (2, 4), (7, 3)]);
    }

    #[test]
    fn side_by_side_goes_row_by_row_left_then_right_across_hunks() {
        // Lines: 0 hunk, 1 context, 2 del, 3 del, 4 add, 5 add, 6 hunk, 7 del, 8 add.
        // Rows: removed and added lines pair up, the context line is on both sides.
        let rows = [
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (Some(2), Some(4)),
            (Some(3), Some(5)),
            (Some(6), Some(6)),
            (Some(7), None),
            (None, Some(8)),
        ];
        let found = vec![
            (1, vec![0..1]),
            (2, vec![0..1]),
            (3, vec![5..6]),
            (4, vec![2..3, 0..1]),
            (5, vec![0..1]),
            (7, vec![0..1]),
            (8, vec![0..1]),
        ];
        let hits = ordered(&found, Layout::Split(&rows));
        let at: Vec<(usize, u8, usize)> = hits.iter().map(|h| (h.row, h.side, h.line)).collect();
        assert_eq!(
            at,
            [
                (1, 0, 1),
                (2, 0, 2),
                (2, 1, 4),
                (2, 1, 4),
                (3, 0, 3),
                (3, 1, 5),
                (5, 0, 7),
                (6, 1, 8),
            ]
        );
        // Within a line, by where they start.
        assert_eq!((hits[2].start, hits[3].start), (0, 2));
        // The same matches unified follow the lines.
        let unified: Vec<usize> = ordered(&found, Layout::Lines)
            .iter()
            .map(|h| h.line)
            .collect();
        assert_eq!(unified, [1, 2, 3, 4, 4, 5, 7, 8]);
    }

    #[test]
    fn nothing_found_orders_nothing() {
        assert!(ordered(&[], Layout::Lines).is_empty());
        assert!(ordered(&[], Layout::Split(&[(Some(0), Some(0))])).is_empty());
    }
}
