//! What the previews are drawn with: text cut to fit, pills, measurements, and the frames of
//! the choices a setting has.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::theme::{self, Tokens};

/// `text` cut with "…" to fit `width` in `font`.
pub(super) fn fit(p: &Painter, text: &str, font: &FontId, width: f32) -> String {
    let w = |s: &str| {
        p.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if w(text) <= width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        let s: String = chars[..n].iter().collect::<String>() + "…";
        if w(&s) <= width {
            return s;
        }
        n -= 1;
    }
    "…".to_owned()
}

/// Text at `pos`, cut to `width`.
#[allow(clippy::too_many_arguments)]
pub(super) fn text(
    p: &Painter,
    pos: Pos2,
    align: Align2,
    s: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Rect {
    let s = fit(p, s, &font, width.max(8.0));
    p.text(pos, align, s, font, color)
}

/// A branch or tag name and the end to keep when the pill is too narrow to show it whole.
pub(super) struct PillLabel<'a> {
    pub text: &'a str,
    pub keep_end: bool,
}

/// Paints a branch or tag pill in its lane's colour, darker unless checked out, and returns
/// its rect.
pub(super) fn pill(
    p: &Painter,
    t: &Tokens,
    left_center: Pos2,
    label: PillLabel<'_>,
    color: Color32,
    head: bool,
    max_w: f32,
) -> Rect {
    let fill = if head {
        color
    } else {
        color.lerp_to_gamma(Color32::BLACK, crate::repo::graph_view::DIMMED)
    };
    let font = if head {
        theme::semibold(11.0)
    } else {
        theme::medium(11.0)
    };
    let label = crate::settings::fit_branch_name(label.text, label.keep_end, |text| {
        p.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= (max_w - 16.0).max(12.0)
    });
    let galley = p.layout_no_wrap(label, font, Color32::WHITE);
    let w = (galley.size().x + 16.0).min(max_w.max(28.0));
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 10.0), vec2(w, 20.0));
    p.rect_filled(r, t.radius, fill);
    p.galley(
        pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0),
        galley,
        Color32::WHITE,
    );
    r
}

/// A measurement from `x0` to `x1` at `y`: a line with ends, and its label above it.
pub(super) fn measure(p: &Painter, t: &Tokens, x0: f32, x1: f32, y: f32, label: &str) {
    let stroke = Stroke::new(1.0, t.accent);
    p.line_segment([pos2(x0, y), pos2(x1, y)], stroke);
    for x in [x0, x1] {
        p.line_segment([pos2(x, y - 4.0), pos2(x, y + 4.0)], stroke);
    }
    let galley = p.layout_no_wrap(label.to_owned(), theme::semibold(10.5), t.accent_text);
    let size = galley.size() + vec2(10.0, 4.0);
    let mid = (x0 + x1) / 2.0;
    let r = Rect::from_center_size(pos2(mid, y), size);
    p.rect_filled(r, t.radius_sm, t.accent);
    p.galley(r.min + vec2(5.0, 2.0), galley, t.accent_text);
}

/// A break across a column drawn narrower than it is: a zigzag from top to bottom.
pub(super) fn zigzag(p: &Painter, t: &Tokens, x: f32, top: f32, bottom: f32, bg: Color32) {
    let band = Rect::from_min_max(pos2(x - 5.0, top), pos2(x + 5.0, bottom));
    p.rect_filled(band, 0.0, bg);
    let mut points = Vec::new();
    let mut y = top;
    let mut left = true;
    while y <= bottom {
        points.push(pos2(if left { x - 4.0 } else { x + 4.0 }, y));
        left = !left;
        y += 6.0;
    }
    let stroke = Stroke::new(1.0, t.text_faint);
    p.add(egui::Shape::line(points.clone(), stroke));
    p.add(egui::Shape::line(
        points.iter().map(|q| *q + vec2(3.0, 0.0)).collect(),
        stroke,
    ));
}

/// The frame of one of a setting's choices drawn side by side: the one set outlined in the
/// accent, with its name under it.
pub(super) fn choice(p: &Painter, t: &Tokens, rect: Rect, name: &str, chosen: bool) {
    let stroke = if chosen {
        Stroke::new(2.0, t.accent)
    } else {
        Stroke::new(1.0, t.field_border)
    };
    // Inside: the choices reach the preview's edges, which would cut an outline outside.
    p.rect_stroke(rect, t.radius, stroke, egui::StrokeKind::Inside);
    let y = rect.bottom() + 10.0;
    let color = if chosen { t.text } else { t.text_faint };
    let font = if chosen {
        theme::semibold(11.5)
    } else {
        FontId::proportional(11.5)
    };
    let label = text(
        p,
        pos2(rect.center().x, y),
        Align2::CENTER_CENTER,
        name,
        font,
        color,
        rect.width(),
    );
    if chosen {
        check(p, pos2(label.left() - 9.0, y), t.accent);
    }
}

/// A check mark centred on `c`.
pub(super) fn check(p: &Painter, c: Pos2, color: Color32) {
    p.add(egui::Shape::line(
        vec![
            c + vec2(-4.0, 0.0),
            c + vec2(-1.0, 3.0),
            c + vec2(4.5, -3.5),
        ],
        Stroke::new(1.8, color),
    ));
}

/// A short bar standing for a line of text.
pub(super) fn bar(p: &Painter, x: f32, y: f32, w: f32, h: f32, color: Color32) {
    p.rect_filled(
        Rect::from_min_size(pos2(x, y - h / 2.0), vec2(w.max(0.0), h)),
        h / 2.0,
        color,
    );
}
