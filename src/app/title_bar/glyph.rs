//! Caption button interaction and native-style glyph painting.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Caption {
    Minimize,
    Maximize,
    Fullscreen,
    Restore,
    Close,
}

impl Caption {
    pub(super) fn tooltip(self) -> &'static str {
        match self {
            Caption::Minimize => crate::i18n::text("interaction-home-minimize"),
            Caption::Maximize => crate::i18n::text("interaction-home-maximize"),
            Caption::Fullscreen => crate::i18n::text("interaction-home-fullscreen"),
            Caption::Restore => crate::i18n::text("interaction-home-restore"),
            Caption::Close => crate::i18n::text("interaction-home-close"),
        }
    }
}

/// A caption button as tall as the strip, drawn as Windows 11 draws its own.
pub(super) fn caption_button(ui: &mut Ui, t: &Tokens, kind: Caption, focused: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(CAPTION_W, ui.available_height()), Sense::click());
    crate::a11y::a11y(&resp, egui::WidgetType::Button, kind.tooltip());
    let pressed = resp.is_pointer_button_down_on();
    let hovered = resp.hovered() || pressed;
    let (fill, glyph) = match kind {
        Caption::Close if pressed => (CLOSE_PRESSED, Color32::WHITE),
        Caption::Close if hovered => (CLOSE_HOVER, Color32::WHITE),
        _ if pressed => (t.text.gamma_multiply(0.05), t.text),
        _ if hovered => (t.text.gamma_multiply(0.09), t.text),
        _ if focused => (Color32::TRANSPARENT, t.text),
        _ => (Color32::TRANSPARENT, t.text_faint),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, fill);
    paint_glyph(painter, rect, kind, glyph);
    resp.on_hover_text(kind.tooltip())
}

/// Windows 11's glyph for `kind`, in the middle of `rect`, in lines one pixel wide.
pub(super) fn paint_glyph(painter: &egui::Painter, rect: Rect, kind: Caption, color: Color32) {
    let ppp = painter.pixels_per_point();
    let px = 1.0 / ppp;
    let stroke = Stroke::new(px, color);
    // Whole pixels, the lines on pixel centres: crisp at any scale.
    let half = (GLYPH_HALF * ppp).round() / ppp;
    let c = pos2(
        painter.round_to_pixel_center(rect.center().x),
        painter.round_to_pixel_center(rect.center().y),
    );
    let (left, right, top, bottom) = (c.x - half, c.x + half, c.y - half, c.y + half);
    match kind {
        Caption::Minimize => {
            painter.line_segment([pos2(left, c.y), pos2(right, c.y)], stroke);
        }
        Caption::Maximize | Caption::Fullscreen => {
            let square = Rect::from_min_max(pos2(left, top), pos2(right, bottom));
            painter.rect_stroke(
                square,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Caption::Restore => {
            // The window in front, and the top and right edges of the one behind it.
            let step = 2.0 * px.max(1.0);
            let front = Rect::from_min_max(pos2(left, top + step), pos2(right - step, bottom));
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.line_segment([pos2(left + step, top), pos2(right, top)], stroke);
            painter.line_segment([pos2(right, top), pos2(right, bottom - step)], stroke);
        }
        Caption::Close => {
            painter.line_segment([pos2(left, top), pos2(right, bottom)], stroke);
            painter.line_segment([pos2(left, bottom), pos2(right, top)], stroke);
        }
    }
}
