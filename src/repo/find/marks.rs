//! The matches marked in a line of text: every one on a soft background, the current one on a
//! strong one with an outline.

use std::ops::Range;

use egui::{Galley, Painter, Pos2, Rangef, Rect, Stroke, StrokeKind, pos2, text::CCursor};

use crate::theme::Tokens;

/// The matches of one line, and which of them is the current one.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineMarks<'a> {
    pub(crate) ranges: &'a [Range<usize>],
    pub(crate) current: Option<usize>,
}

/// Marks the matches of a line laid out in `galley` from `origin`, between the heights `y`.
/// Painted before the text, so the text stays on top.
pub(crate) fn paint_marks(
    painter: &Painter,
    origin: Pos2,
    galley: &Galley,
    y: Rangef,
    marks: LineMarks<'_>,
    t: &Tokens,
) {
    let at = |c: usize| origin.x + galley.pos_from_cursor(CCursor::new(c)).min.x;
    for (k, chars) in marks.ranges.iter().enumerate() {
        let rect = Rect::from_x_y_ranges(
            Rangef::new(at(chars.start), at(chars.end).max(at(chars.start) + 2.0)),
            Rangef::new(y.min + 1.0, y.max - 1.0),
        );
        if marks.current == Some(k) {
            painter.rect_filled(rect, 2.0, t.warning.gamma_multiply(0.6));
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0, t.warning), StrokeKind::Outside);
        } else {
            painter.rect_filled(rect, 2.0, t.warning.gamma_multiply(0.28));
        }
    }
}

/// The match marks of a line painted from `x`, centred at `mid` in a row of height `h`; then
/// the line itself. For the plain views (a file, its blame).
pub(crate) fn paint_line(
    painter: &Painter,
    (x, mid, h): (f32, f32, f32),
    galley: std::sync::Arc<Galley>,
    marks: Option<LineMarks<'_>>,
    t: &Tokens,
) {
    let origin = pos2(x, mid - galley.size().y / 2.0);
    if let Some(marks) = marks {
        let y = Rangef::new(mid - h / 2.0, mid + h / 2.0);
        paint_marks(painter, origin, &galley, y, marks, t);
    }
    painter.galley(origin, galley, t.text);
}
