//! A theme drawn small: the whole window in the preview, and a thumbnail of it in the gallery.

use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};

use super::{Preview, area, draw};
use crate::theme::{self, Tokens};

/// The window in `t`'s colours: the tab strip, a toolbar, the branches, the graph, a diff, a
/// commit box and the terminal. `preview` says what to point out.
pub(super) fn window(ui: &mut Ui, t: &Tokens, width: f32, preview: Preview) {
    let height = (width * 0.72).clamp(220.0, 320.0);
    let rect = area(ui, width, height);
    let p = ui.painter_at(rect);
    let r = rect;
    p.rect_filled(r, t.radius_lg, t.dock);

    // Tab strip with two tabs.
    let strip = Rect::from_min_size(r.min, vec2(r.width(), 24.0));
    p.rect_filled(
        strip,
        egui::CornerRadius {
            nw: t.radius_lg as u8,
            ne: t.radius_lg as u8,
            sw: 0,
            se: 0,
        },
        t.tab_strip,
    );
    let tab = Rect::from_min_size(strip.min + vec2(8.0, 4.0), vec2(96.0, 20.0));
    p.rect_filled(
        tab,
        egui::CornerRadius {
            nw: t.radius as u8,
            ne: t.radius as u8,
            sw: 0,
            se: 0,
        },
        t.chrome,
    );
    p.text(
        tab.left_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        "gitcraft",
        theme::medium(11.0),
        t.text,
    );
    p.text(
        pos2(tab.right() + 14.0, tab.center().y),
        Align2::LEFT_CENTER,
        "website",
        FontId::proportional(11.0),
        t.text_faint,
    );

    // Toolbar: a few buttons, the primary one in the accent.
    let bar = Rect::from_min_size(pos2(r.left(), strip.bottom()), vec2(r.width(), 30.0));
    p.rect_filled(bar, 0.0, t.chrome);
    p.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let mut x = bar.left() + 10.0;
    for (w, hovered) in [(36.0, false), (40.0, true), (34.0, false)] {
        let b = Rect::from_min_size(pos2(x, bar.top() + 6.0), vec2(w, 18.0));
        if hovered {
            p.rect_filled(b, t.radius_sm, t.hover);
        }
        draw::bar(&p, b.left() + 8.0, b.center().y, w - 16.0, 4.0, t.icon);
        x += w + 6.0;
    }
    let push = Rect::from_min_size(pos2(bar.right() - 64.0, bar.top() + 5.0), vec2(54.0, 20.0));
    p.rect_filled(push, t.radius_sm, t.accent);
    p.text(
        push.center(),
        Align2::CENTER_CENTER,
        "Push",
        theme::semibold(11.0),
        t.accent_text,
    );
    if preview == Preview::Accent {
        p.rect_stroke(
            push.expand(3.0),
            t.radius,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Outside,
        );
    }

    // The terminal along the bottom.
    let term = Rect::from_min_max(pos2(r.left(), r.bottom() - 44.0), r.max);
    // Branches on the left, details on the right, the graph between.
    let side_w = (r.width() * 0.25).max(70.0);
    let detail_w = (r.width() * 0.32).max(90.0);
    let mid_top = bar.bottom();
    let side = Rect::from_min_max(pos2(r.left(), mid_top), pos2(r.left() + side_w, term.top()));
    let detail = Rect::from_min_max(
        pos2(r.right() - detail_w, mid_top),
        pos2(r.right(), term.top()),
    );
    let graph = Rect::from_min_max(pos2(side.right(), mid_top), pos2(detail.left(), term.top()));

    p.rect_filled(side, 0.0, t.card);
    p.line_segment(
        [side.right_top(), side.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    p.text(
        side.left_top() + vec2(10.0, 12.0),
        Align2::LEFT_CENTER,
        "BRANCHES",
        theme::semibold(9.0),
        t.text_faint,
    );
    for (i, (name, selected)) in [
        ("main", false),
        ("feature/themes", true),
        ("fix/tabs", false),
    ]
    .into_iter()
    .enumerate()
    {
        let y = side.top() + 30.0 + i as f32 * 20.0;
        let row = Rect::from_min_size(
            pos2(side.left() + 4.0, y - 9.0),
            vec2(side.width() - 8.0, 18.0),
        );
        if selected {
            p.rect_filled(row, t.radius_sm, t.row_selected);
        }
        p.circle_filled(pos2(row.left() + 9.0, y), 3.0, t.lane(i));
        draw::text(
            &p,
            pos2(row.left() + 18.0, y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(10.5),
            if selected { t.text } else { t.text_dim },
            row.width() - 22.0,
        );
    }

    // The graph: lanes, dots, pills, descriptions.
    let lanes = [0usize, 1, 0, 2, 0];
    let summaries = [
        "Theme gallery",
        "Merge fix/tabs",
        "Previews in the settings",
        "Tabs scroll",
        "Release 0.9",
    ];
    let pitch = ((graph.height() - 8.0) / lanes.len() as f32).min(24.0);
    let lane_x = |lane: usize| graph.left() + 14.0 + lane as f32 * 12.0;
    for lane in 0..3 {
        p.line_segment(
            [
                pos2(lane_x(lane), graph.top() + 4.0 + pitch / 2.0),
                pos2(lane_x(lane), graph.bottom()),
            ],
            Stroke::new(
                1.6,
                t.lane(lane)
                    .gamma_multiply(if lane == 0 { 1.0 } else { 0.85 }),
            ),
        );
    }
    for (i, (&lane, summary)) in lanes.iter().zip(summaries).enumerate() {
        let y = graph.top() + 4.0 + pitch * (i as f32 + 0.5);
        if i == 2 {
            p.rect_filled(
                Rect::from_min_size(
                    pos2(graph.left(), y - pitch / 2.0 + 1.0),
                    vec2(graph.width(), pitch - 2.0),
                ),
                0.0,
                t.hover,
            );
        }
        p.circle_filled(pos2(lane_x(lane), y), 4.5, t.lane(lane));
        let tx = lane_x(2) + 14.0;
        draw::text(
            &p,
            pos2(tx, y),
            Align2::LEFT_CENTER,
            summary,
            FontId::proportional(10.5),
            t.text,
            graph.right() - tx - 6.0,
        );
    }

    // Details: a diff and the commit box.
    p.rect_filled(detail, 0.0, t.card);
    p.line_segment(
        [detail.left_top(), detail.left_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let dx = detail.left() + 8.0;
    let dw = detail.width() - 16.0;
    let code = Rect::from_min_size(pos2(dx, detail.top() + 8.0), vec2(dw, 52.0));
    p.rect_filled(code, t.radius_sm, t.field);
    let line = |i: usize| code.top() + 7.0 + i as f32 * 13.0;
    let rows: [(Option<Color32>, Color32, &str); 4] = [
        (None, t.text_dim, "  let t = theme;"),
        (Some(t.diff_del_bg), t.danger, "- paint(old);"),
        (Some(t.diff_add_bg), t.success, "+ paint(new);"),
        (None, t.text_dim, "  }"),
    ];
    for (i, (bg, fg, text)) in rows.into_iter().enumerate() {
        let y = line(i);
        if let Some(bg) = bg {
            p.rect_filled(
                Rect::from_min_size(pos2(code.left(), y - 6.0), vec2(code.width(), 12.0)),
                0.0,
                bg,
            );
        }
        draw::text(
            &p,
            pos2(code.left() + 5.0, y),
            Align2::LEFT_CENTER,
            text,
            theme::mono(9.5),
            fg,
            code.width() - 8.0,
        );
    }
    let field = Rect::from_min_size(pos2(dx, code.bottom() + 10.0), vec2(dw, 22.0));
    let field_stroke = if preview == Preview::Accent {
        Stroke::new(1.0, t.accent)
    } else {
        Stroke::new(1.0, t.field_border)
    };
    p.rect_filled(field, t.radius_sm, t.field);
    p.rect_stroke(field, t.radius_sm, field_stroke, egui::StrokeKind::Inside);
    draw::text(
        &p,
        field.left_center() + vec2(6.0, 0.0),
        Align2::LEFT_CENTER,
        "Commit message",
        FontId::proportional(10.0),
        t.text_faint,
        field.width() - 10.0,
    );
    let commit = Rect::from_min_size(pos2(dx, field.bottom() + 6.0), vec2(dw, 20.0));
    if commit.bottom() < detail.bottom() - 2.0 {
        p.rect_filled(commit, t.radius_sm, t.accent);
        p.text(
            commit.center(),
            Align2::CENTER_CENTER,
            "Commit",
            theme::semibold(10.5),
            t.accent_text,
        );
    }

    // Terminal: a prompt and the 16 colours.
    p.rect_filled(
        term,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: t.radius_lg as u8,
            se: t.radius_lg as u8,
        },
        t.field,
    );
    p.line_segment(
        [term.left_top(), term.right_top()],
        Stroke::new(1.0, t.separator),
    );
    let y = term.top() + 13.0;
    let mut x = term.left() + 10.0;
    for (text, color) in [
        ("~/gitcraft", t.ansi[4]),
        (" (main)", t.ansi[5]),
        (" $ ", t.text),
        ("git status", t.text),
    ] {
        let r = p.text(
            pos2(x, y),
            Align2::LEFT_CENTER,
            text,
            theme::mono(10.0),
            color,
        );
        x = r.right();
    }
    let sw = ((term.width() - 20.0) / 16.0).min(14.0);
    for (i, c) in t.ansi.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(term.left() + 10.0 + i as f32 * sw, term.top() + 26.0),
            vec2(sw - 2.0, 10.0),
        );
        p.rect_filled(r, 2.0, *c);
    }
    p.rect_stroke(
        rect,
        t.radius_lg,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
}

/// Buttons, a field, pills and a panel twice their size, to see their corners.
pub(super) fn corners(ui: &mut Ui, t: &Tokens, width: f32) {
    const ZOOM: f32 = 2.0;
    let rect = area(ui, width, 120.0);
    let p = ui.painter_at(rect);
    let card = rect.shrink(1.0);
    p.rect_filled(card, t.radius_lg * ZOOM, t.card);
    p.rect_stroke(
        card,
        t.radius_lg * ZOOM,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    let y = card.top() + 34.0;
    let button = Rect::from_min_size(pos2(card.left() + 16.0, y - 18.0), vec2(110.0, 36.0));
    p.rect_filled(button, t.radius_sm * ZOOM, t.accent);
    p.text(
        button.center(),
        Align2::CENTER_CENTER,
        "Commit",
        theme::semibold(16.0),
        t.accent_text,
    );
    let field = Rect::from_min_max(
        pos2(button.right() + 14.0, y - 18.0),
        pos2(card.right() - 16.0, y + 18.0),
    );
    p.rect_filled(field, t.radius_sm * ZOOM, t.field);
    p.rect_stroke(
        field,
        t.radius_sm * ZOOM,
        Stroke::new(1.5, t.field_border),
        egui::StrokeKind::Inside,
    );
    draw::text(
        &p,
        field.left_center() + vec2(12.0, 0.0),
        Align2::LEFT_CENTER,
        "Summary",
        FontId::proportional(15.0),
        t.text_faint,
        field.width() - 20.0,
    );
    let y = card.bottom() - 30.0;
    let mut x = card.left() + 16.0;
    for (i, name) in ["main", "feature/x"].iter().enumerate() {
        let galley = p.layout_no_wrap((*name).to_owned(), theme::semibold(15.0), Color32::WHITE);
        let pill = Rect::from_min_size(pos2(x, y - 15.0), vec2(galley.size().x + 24.0, 30.0));
        p.rect_filled(
            pill,
            t.radius * ZOOM,
            theme::palette::white_text_fill(t.lane(i)),
        );
        p.galley(
            pill.min + vec2(12.0, 15.0 - galley.size().y / 2.0),
            galley,
            Color32::WHITE,
        );
        x = pill.right() + 10.0;
    }
    p.text(
        pos2(card.right() - 16.0, y),
        Align2::RIGHT_CENTER,
        format!(
            "{:.0} · {:.0} · {:.0} px",
            t.radius_sm, t.radius, t.radius_lg
        ),
        FontId::proportional(11.5),
        t.text_faint,
    );
    ui.add_space(10.0);
}

/// A theme's thumbnail in the gallery: the window as in [`window`], in a few strokes.
pub(crate) fn thumbnail(p: &Painter, rect: Rect, t: &Tokens) {
    let r = rect;
    let radius = 6.0;
    p.rect_filled(r, radius, t.dock);
    let strip = Rect::from_min_size(r.min, vec2(r.width(), 10.0));
    p.rect_filled(
        strip,
        egui::CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        },
        t.tab_strip,
    );
    p.rect_filled(
        Rect::from_min_size(strip.min + vec2(6.0, 3.0), vec2(26.0, 7.0)),
        egui::CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        t.chrome,
    );
    let bar = Rect::from_min_size(pos2(r.left(), strip.bottom()), vec2(r.width(), 10.0));
    p.rect_filled(bar, 0.0, t.chrome);
    p.rect_filled(
        Rect::from_min_size(pos2(bar.right() - 20.0, bar.top() + 2.5), vec2(15.0, 5.0)),
        1.5,
        t.accent,
    );
    let body_top = bar.bottom();
    let side = Rect::from_min_max(
        pos2(r.left(), body_top),
        pos2(r.left() + r.width() * 0.26, r.bottom()),
    );
    p.rect_filled(
        side,
        egui::CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius as u8,
            se: 0,
        },
        t.card,
    );
    for i in 0..4 {
        let y = side.top() + 8.0 + i as f32 * 9.0;
        if i == 1 {
            p.rect_filled(
                Rect::from_min_size(
                    pos2(side.left() + 2.0, y - 3.5),
                    vec2(side.width() - 4.0, 7.0),
                ),
                1.5,
                t.row_selected,
            );
        }
        draw::bar(
            p,
            side.left() + 5.0,
            y,
            side.width() * 0.6,
            2.5,
            t.text_faint,
        );
    }
    let gx = side.right() + 8.0;
    for lane in 0..3 {
        let x = gx + lane as f32 * 7.0;
        p.line_segment(
            [pos2(x, body_top + 6.0), pos2(x, r.bottom() - 4.0)],
            Stroke::new(1.2, t.lane(lane)),
        );
    }
    for (i, lane) in [0usize, 1, 0, 2, 0].into_iter().enumerate() {
        let y = body_top + 7.0 + i as f32 * 9.0;
        if y > r.bottom() - 4.0 {
            break;
        }
        p.circle_filled(pos2(gx + lane as f32 * 7.0, y), 2.4, t.lane(lane));
        let w = (r.right() - gx - 30.0) * [0.8, 0.55, 0.7, 0.5, 0.65][i];
        draw::bar(p, gx + 22.0, y, w, 2.5, t.text.gamma_multiply(0.8));
    }
    p.rect_stroke(
        r,
        radius,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
}
