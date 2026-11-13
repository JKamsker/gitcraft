//! A row of the _All repositories_ list: the name (and, for a Recent one, its branch), the
//! path, where it was found, and _Hide from library_ for one a scan found.

use std::path::Path;

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::rows::Row;
use crate::app::home_recent::{elided, first_line};
use crate::app::recent_status::RecentStatus;
use crate::icons;
use crate::theme::Tokens;

pub(in crate::app) enum RowClick {
    Open,
    Hide,
}

/// Paints `row` with its `tag` (where it was found). Returns what was clicked, and whether
/// the mouse is over it.
pub(in crate::app) fn library_row(
    ui: &mut Ui,
    row: &Row,
    tag: &str,
    status: Option<&RecentStatus>,
    highlighted: bool,
) -> (Option<RowClick>, bool) {
    let t = Tokens::get(ui.ctx());
    let path: &Path = &row.path;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    crate::repo::widgets::a11y(&resp, egui::WidgetType::Button, || {
        crate::remote::display(path)
    });
    let hovered = resp.hovered();
    if hovered || highlighted {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    if highlighted {
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0));
        ui.painter().rect_filled(bar, 1.5, t.accent);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect,
            t.radius,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 20.0, y), vec2(18.0, 18.0)),
        "folder-git-2",
        16.0,
        if row.stale { t.text_faint } else { t.icon },
    );
    let left = rect.left() + 40.0;
    let right = rect.right() - 36.0;
    let p = ui.painter_at(rect.with_max_x(right));
    let tag_color = if row.stale { t.warning } else { t.text_faint };
    let tag_left = p
        .text(
            pos2(right, y - 8.0),
            Align2::RIGHT_CENTER,
            tag,
            egui::FontId::proportional(11.0),
            tag_color,
        )
        .left()
        - 12.0;
    first_line(ui, &p, path, status, pos2(left, y - 8.0), tag_left);
    let shown = elided(
        ui,
        crate::remote::display(path),
        egui::FontId::proportional(11.0),
        t.text_faint,
        right - left,
    );
    p.galley(
        pos2(left, y + 8.0 - shown.size().y / 2.0),
        shown,
        t.text_faint,
    );
    let mut hide = false;
    let hide_id = resp.id.with("hide");
    let keyboard = crate::a11y::keyboard_on(&resp) || ui.memory(|m| m.has_focus(hide_id));
    if row.hideable() && (hovered || keyboard || ui.rect_contains_pointer(rect)) {
        let label = crate::i18n::text("interaction-library-hide");
        let x_rect = Rect::from_center_size(pos2(rect.right() - 18.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, hide_id, Sense::click())
            .on_hover_text(crate::i18n::text("interaction-library-hide-tip"));
        crate::repo::widgets::a11y(&x, egui::WidgetType::Button, label);
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "eye-off", 12.0, t.text_faint);
        hide = x.clicked();
    }
    let clicked = if hide {
        Some(RowClick::Hide)
    } else if resp.clicked() {
        Some(RowClick::Open)
    } else {
        None
    };
    (clicked, hovered)
}
