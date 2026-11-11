//! Presentation coordinates and selection transitions. Patch indices stay distinct from
//! displayed rows; wrapping adds pixel geometry without changing either index space.
use super::{DiffData, DiffLine, Kind, TextSel};
use crate::repo::DiffView;

/// An explicit borrowed map: unified rows map to one patch line, split rows to two.
/// Borrowing the data prevents a map surviving fold insertion/removal or a reload.
pub(super) struct DisplayRows<'a> {
    lines: &'a [DiffLine],
    split: Option<&'a [(Option<usize>, Option<usize>)]>,
}

impl<'a> DisplayRows<'a> {
    pub(super) fn new(data: &'a DiffData, split: bool) -> Self {
        Self {
            lines: &data.lines,
            split: split.then_some(&data.split),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.split.map_or(self.lines.len(), |rows| rows.len())
    }

    pub(super) fn patch_lines(&self, row: usize) -> Option<(Option<usize>, Option<usize>)> {
        match self.split {
            Some(rows) => rows.get(row).copied(),
            None => self.lines.get(row).map(|_| (Some(row), Some(row))),
        }
    }

    pub(super) fn row_of(&self, line: usize) -> Option<usize> {
        match self.split {
            Some(rows) => rows
                .iter()
                .position(|&(a, b)| a == Some(line) || b == Some(line)),
            None => self.lines.get(line).map(|_| line),
        }
    }
}

/// Only this owner translates pointer intent into persistent text/action selection.
/// The fields stay on DiffView for compatibility with keyboard, reset and fold callers.
pub(super) fn select(
    view: &mut DiffView,
    text: Option<TextSel>,
    clear: bool,
    text_started: bool,
    clicked: Option<usize>,
    extend: bool,
) {
    view.text_sel = text;
    if clear || text_started {
        view.chosen.clear();
        view.anchor = None;
    }
    if let Some(i) = clicked {
        pick_line(view, i, extend);
    }
}

fn pick_line(view: &mut DiffView, i: usize, extend: bool) {
    let Some(Ok(data)) = &view.data else { return };
    if !data
        .lines
        .get(i)
        .is_some_and(|l| matches!(l.kind, Kind::Add | Kind::Del))
    {
        return;
    }
    match view.anchor {
        Some(a) if extend => {
            view.chosen.extend((a.min(i)..=a.max(i)).filter(|&k| {
                data.lines
                    .get(k)
                    .is_some_and(|l| matches!(l.kind, Kind::Add | Kind::Del))
            }));
        }
        _ => {
            if !view.chosen.remove(&i) {
                view.chosen.insert(i);
            }
            view.anchor = Some(i);
        }
    }
}

/// Fold edits invalidate all derived row tables together, in both layouts.
pub(super) fn invalidate_rows(data: &mut DiffData) {
    // Source hashes and row counts can survive replacement of the final fold.
    // A process-wide revision also distinguishes separate copies of the same source.
    static REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    data.geometry_revision = REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    data.map = Default::default();
    data.hunks = Default::default();
}

/// Reload and fold boundaries must discard physical geometry from the previous rows.
pub(super) fn reset_geometry(view: &mut DiffView) {
    view.wrap_offsets = None;
    view.keyboard_hunk = None;
    view.at_change = None;
}

/// Keep persistent patch-line selections on their original lines after a fold edit.
pub(super) fn follow_rows(view: &mut DiffView, shift: super::context::Shift) {
    let row = |i: usize| shift.row(i);
    let keyboard_hunk = view.keyboard_hunk.map(row);
    reset_geometry(view);
    view.keyboard_hunk = keyboard_hunk;
    view.chosen = view.chosen.iter().map(|&i| row(i)).collect();
    view.anchor = view.anchor.map(row);
    view.jump_line = view.jump_line.map(row);
    view.text_sel = view.text_sel.map(|mut s| {
        s.anchor.0 = row(s.anchor.0);
        s.cursor.0 = row(s.cursor.0);
        s
    });
}

/// Rendered layout identity, kept beside the UI rather than on patch selections.
#[derive(Clone, PartialEq)]
pub(super) struct KeyboardLayout {
    pub key: egui::Id,
    pub offsets: Option<std::sync::Arc<Vec<f32>>>,
    pub viewport_height: f32,
}

impl KeyboardLayout {
    pub(super) fn changed(&self, ctx: &egui::Context, id: egui::Id) -> bool {
        ctx.data_mut(|d| {
            let changed = d.get_temp::<Self>(id).as_ref() != Some(self);
            d.insert_temp(id, self.clone());
            changed
        })
    }
}

/// Pointer intent wins this frame; reflow refreshes only the fallback hunk.
/// Explicit text and Git-line selections remain patch indices and are untouched.
pub(super) fn keyboard_target(
    view: &DiffView,
    data: &DiffData,
    split: bool,
    offsets: Option<&[f32]>,
    viewport: std::ops::Range<f32>,
    focused_line: Option<usize>,
    moved: bool,
) -> Option<usize> {
    if let Some(line) = focused_line {
        data.hunk_at(false, line)
    } else if view.keyboard_hunk.is_none() || moved {
        scroll_hunk(data, split, offsets, viewport)
    } else {
        view.keyboard_hunk
    }
}

/// Look ahead three ordinary line heights, bounded by the physical viewport.
/// A header above the viewport still owns its context; a header below it cannot win.
pub(super) fn scroll_hunk(
    data: &DiffData,
    split: bool,
    offsets: Option<&[f32]>,
    viewport: std::ops::Range<f32>,
) -> Option<usize> {
    if viewport.end <= viewport.start {
        return None;
    }
    let y = (viewport.start + 3.0 * super::LINE_H).min(viewport.end.next_down());
    let logical = super::wrap::logical_offset(offsets, y);
    let hunk = data.hunk_at(split, (logical / super::LINE_H) as usize)?;
    let row = DisplayRows::new(data, split).row_of(hunk)?;
    let top = offsets.map_or(row as f32 * super::LINE_H, |o| {
        *o.get(row).unwrap_or(&f32::INFINITY)
    });
    (top < viewport.end).then_some(hunk)
}

// Compatibility for existing pointer harnesses; production transitions use select().
#[cfg(test)]
impl DiffView {
    pub(in crate::repo::diff) fn pick_line(&mut self, i: usize, extend: bool) {
        pick_line(self, i, extend);
    }
}

#[cfg(test)]
#[path = "presentation/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "presentation/layout_regressions.rs"]
mod layout_regressions;
