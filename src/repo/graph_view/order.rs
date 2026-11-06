//! The graph column's name in the header, as a button: it opens the menu of the order history
//! is read in and _First parent only_.

use egui::{Rect, Sense, Ui, pos2};

use super::header;
use crate::git::LogOrder;
use crate::icons;
use crate::repo::columns::Cols;
use crate::repo::{Action, graph_order};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// Clear of the handles that resize the columns on either side.
const INSET: f32 = 6.0;
const CHEVRON: f32 = 11.0;

/// Over the graph column's name (`label`, painted by the header) in `header`: a chevron, and a
/// click opens the order menu. A right-click still opens the header's own menu.
pub(super) fn order_button(
    ui: &mut Ui,
    header: Rect,
    cols: &Cols,
    (label, current): (&str, LogOrder),
    settings: &Settings,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let left = header.left() + cols.refs + INSET;
    let right = header.left() + cols.refs + cols.graph - INSET;
    let text_w = ui
        .painter()
        .layout_no_wrap(label.to_owned(), theme::semibold(10.5), t.text_faint)
        .size()
        .x;
    let end = (left + 4.0 + text_w + 3.0 + CHEVRON + 4.0).min(right);
    if end - left < CHEVRON {
        return;
    }
    let rect = Rect::from_min_max(
        pos2(left, header.top() + 3.0),
        pos2(end, header.bottom() - 3.0),
    );
    let resp = ui
        .interact(rect, ui.id().with("graph-order"), Sense::click())
        .on_hover_text("Order history, or show first parents only");
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, t.radius_sm, t.text_faint.gamma_multiply(0.12));
    }
    let chevron = Rect::from_center_size(
        pos2(end - 4.0 - CHEVRON / 2.0, header.center().y),
        egui::vec2(CHEVRON, CHEVRON),
    );
    let tint = if current.is_default() {
        t.text_faint
    } else {
        t.accent
    };
    // Over the end of a label too long for the column.
    ui.painter().rect_filled(chevron.expand(2.0), 0.0, t.chrome);
    icons::paint(ui, chevron, "chevron-down", CHEVRON, tint);
    egui::Popup::menu(&resp)
        .style(theme::menu_style)
        .show(|ui| graph_order::menu_items(ui, current, acts));
    header::header_menu(&resp, settings);
}
