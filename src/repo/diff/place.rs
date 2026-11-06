//! Keeping one's place when a diff is read again another way (another algorithm): the line at
//! the top of the view stays at the top, wherever the new reading puts its row.

use super::{DiffData, LINE_H};

/// A line of the file, by its numbers before and after the change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Place {
    old: Option<u32>,
    new: Option<u32>,
}

/// The rows' lines, unified or side by side (`split`): each row's (old, new) line numbers.
fn row_numbers(data: &DiffData, split: bool, row: usize) -> (Option<u32>, Option<u32>) {
    let line = |i: Option<usize>| i.and_then(|i| data.lines.get(i));
    if split {
        let Some(&(left, right)) = data.split.get(row) else {
            return (None, None);
        };
        let (left, right) = (line(left), line(right));
        (
            left.and_then(|l| l.old).or(right.and_then(|l| l.old)),
            right.and_then(|l| l.new).or(left.and_then(|l| l.new)),
        )
    } else {
        line(Some(row)).map_or((None, None), |l| (l.old, l.new))
    }
}

fn row_count(data: &DiffData, split: bool) -> usize {
    if split {
        data.split.len()
    } else {
        data.lines.len()
    }
}

/// The place shown at the top of a view scrolled to `offset`: the first line with a number
/// from the top row down (a hunk's header has none).
pub(in crate::repo) fn place_at(data: &DiffData, split: bool, offset: f32) -> Option<Place> {
    let top = (offset.max(0.0) / LINE_H).floor() as usize;
    (top..row_count(data, split)).find_map(|row| match row_numbers(data, split, row) {
        (None, None) => None,
        (old, new) => Some(Place { old, new }),
    })
}

/// The scroll offset that puts `place` back at the top in `data`: the first row at or past
/// its line of the new side (or the old side, for a removed line).
pub(in crate::repo) fn offset_of_place(data: &DiffData, split: bool, place: Place) -> f32 {
    let at = |row: usize| {
        let (old, new) = row_numbers(data, split, row);
        match (place.new, place.old) {
            (Some(want), _) if new.is_some() => new.is_some_and(|n| n >= want),
            (_, Some(want)) => old.is_some_and(|o| o >= want),
            (Some(want), None) => new.is_some_and(|n| n >= want),
            (None, None) => false,
        }
    };
    let row = (0..row_count(data, split)).find(|&r| at(r)).unwrap_or(0);
    row as f32 * LINE_H
}
