//! The lines of each view the find bar searches.

use super::{Find, Hit, Layout, Source, SourceKey};
use crate::repo::diff::{DiffData, Kind};

/// Searches the diff's lines for the find bar (its hunk headers and notes aren't); the
/// match to scroll to this frame.
pub(crate) fn search_diff(
    find: &mut Find,
    ctx: &egui::Context,
    request: u64,
    data: &DiffData,
    split: bool,
) -> Option<Hit> {
    let lines = &data.lines;
    let key = SourceKey {
        what: Source::Diff(request),
        at: lines.as_ptr() as usize,
        len: lines.len(),
    };
    let layout = if split {
        Layout::Split(&data.split)
    } else {
        Layout::Lines
    };
    let text = |i: usize| {
        lines
            .get(i)
            .filter(|l| matches!(l.kind, Kind::Context | Kind::Add | Kind::Del))
            .map(|l| l.text.as_str())
    };
    find.update(ctx, key, lines.len(), text, layout);
    find.take_reveal()
}
