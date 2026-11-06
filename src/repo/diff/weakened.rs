//! The _weakens checks_ badge on a diff's hunks (see [`crate::weakened`]).

use std::collections::HashMap;

use egui::{CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::{DiffLine, Kind};
use crate::theme::{self, Tokens};

/// The evidence of each hunk that weakens the checks in the one-file diff `text`, by the line
/// of `lines` its header is. What concerns the whole file (a test file deleted, a snapshot
/// rewritten) goes on the first hunk.
pub(super) fn by_hunk_line(text: &str, lines: &[DiffLine]) -> HashMap<usize, String> {
    let Some(file) = crate::weakened::scan_patch(text).into_iter().next() else {
        return HashMap::new();
    };
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind == Kind::Hunk)
        .map(|(i, _)| i)
        .collect();
    let mut out = HashMap::new();
    for (place, &line) in headers.iter().enumerate() {
        let flags = file.flags.iter().filter(|f| f.hunk.unwrap_or(0) == place);
        let tip = crate::weakened::summary(flags);
        if !tip.is_empty() {
            out.insert(line, tip);
        }
    }
    out
}

/// The badge right-aligned at `right` on the header of hunk `i`, `tip` on hover. Opaque, so
/// the header's text under it doesn't show through.
pub(super) fn badge(ui: &mut Ui, right: f32, y: f32, tip: &str, i: usize) {
    let t = Tokens::get(ui.ctx());
    let color = crate::repo::widgets::badge_text(&t);
    let galley = ui.painter().layout_no_wrap(
        crate::weakened::BADGE.to_owned(),
        theme::medium(10.0),
        color,
    );
    let size = galley.size() + vec2(10.0, 3.0);
    let rect = Rect::from_min_size(pos2(right - size.x, y - size.y / 2.0), size);
    let painter = ui.painter();
    painter.rect_filled(rect.expand(3.0), CornerRadius::ZERO, t.dock);
    painter.rect_filled(rect, CornerRadius::same(4), t.warning.gamma_multiply(0.18));
    painter.galley(rect.min + vec2(5.0, 1.5), galley, color);
    ui.interact(rect, ui.id().with(("weakened-hunk", i)), Sense::hover())
        .on_hover_text(tip);
}
