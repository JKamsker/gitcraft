//! Loading and ghost connector row painting.

use super::*;

/// The last row while older history is still loading.
pub(super) fn loading_row(ui: &mut Ui, t: &Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_PITCH), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        crate::i18n::text("repo-core-4142"),
        FontId::proportional(12.0),
        t.text_faint,
    );
}

/// Right-click menu of the changes row.
/// A collapsed run preserves only continuous lanes at its boundaries. Ended/reused lanes
/// have separate caps; hidden internal merges are deliberately not invented as edges.
pub(super) fn connector_row(
    ui: &mut Ui,
    t: &Tokens,
    cols: Cols,
    lanes: Lanes,
    count: usize,
    top: &[Option<usize>; 24],
    bottom: &[Option<usize>; 24],
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_PITCH), Sense::click());
    let painter = ui.painter_at(rect);
    let mid = rect.center().y;
    for col in 0..lanes.shown.min(24) {
        let x = rect.left() + cols.refs + 6.0 + col as f32 * lanes.width + lanes.width / 2.0;
        let stroke = egui::Stroke::new(1.3, t.lane(col).gamma_multiply(0.65));
        let continuous = top[col].is_some() && top[col] == bottom[col];
        for step in 0..6 {
            let y = rect.top() + step as f32 * 5.0;
            let upper = y < mid - 3.0;
            let lower = y > mid + 1.0;
            if continuous || (upper && top[col].is_some()) || (lower && bottom[col].is_some()) {
                painter.line_segment(
                    [
                        egui::pos2(x, y),
                        egui::pos2(x, (y + 3.0).min(rect.bottom())),
                    ],
                    stroke,
                );
            }
        }
        if !continuous {
            if top[col].is_some() {
                painter.line_segment(
                    [
                        egui::pos2(x - 2.0, mid - 3.0),
                        egui::pos2(x + 2.0, mid - 3.0),
                    ],
                    stroke,
                );
            }
            if bottom[col].is_some() {
                painter.line_segment(
                    [
                        egui::pos2(x - 2.0, mid + 3.0),
                        egui::pos2(x + 2.0, mid + 3.0),
                    ],
                    stroke,
                );
            }
        }
    }
    painter.text(
        egui::pos2(rect.left() + cols.refs + cols.graph + 6.0, mid),
        Align2::LEFT_CENTER,
        crate::i18n::message("repo-core-4147", &[("count", (count).into())]),
        FontId::proportional(12.0),
        if response.hovered() {
            t.text
        } else {
            t.text_dim
        },
    );
    response.on_hover_text(crate::i18n::text("repo-core-4148"))
}
