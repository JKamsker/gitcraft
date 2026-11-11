//! Home action cards and bounded subtitle layout.

use super::*;

/// A large clickable card on the start page.
pub(super) fn action_card(
    ui: &mut Ui,
    width: f32,
    icon: &str,
    title: &str,
    subtitle: &str,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 122.0), Sense::click());
    widgets::a11y(&resp, WidgetType::Button, || {
        labels::join([title, subtitle])
    });
    let fill = if resp.hovered() { t.hover } else { t.card };
    let stroke = if resp.hovered() || resp.has_focus() {
        t.accent
    } else {
        t.field_border
    };
    let p = ui.painter();
    p.rect(
        rect,
        t.radius_lg,
        fill,
        Stroke::new(1.0, stroke),
        egui::StrokeKind::Inside,
    );
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.center().x, rect.top() + 34.0), vec2(28.0, 28.0)),
        icon,
        26.0,
        t.accent,
    );
    p.text(
        pos2(rect.center().x, rect.top() + 68.0),
        Align2::CENTER_CENTER,
        title,
        theme::semibold(14.0),
        t.text,
    );
    // Wrapped (two lines at most, then cut with "…") inside the card, clear of its sides.
    let galley = p.layout_job(subtitle_job(subtitle, width, t.text_dim));
    let top = rect.top() + 82.0;
    let pos = pos2(
        rect.center().x - galley.rect.center().x,
        top - galley.rect.top(),
    );
    p.galley(pos, galley, t.text_dim);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An action card's subtitle, laid out to fit a card `card_width` wide.
pub(super) fn subtitle_job(
    subtitle: &str,
    card_width: f32,
    color: egui::Color32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(
        subtitle.to_owned(),
        egui::TextFormat::simple(egui::FontId::proportional(11.5), color),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: subtitle_width(card_width),
        max_rows: 2,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job.halign = egui::Align::Center;
    job
}

/// How wide an action card's subtitle may be.
pub(super) fn subtitle_width(card_width: f32) -> f32 {
    (card_width - 2.0 * CARD_TEXT_MARGIN).max(1.0)
}
