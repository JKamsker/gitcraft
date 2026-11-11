//! Diff layout and context previews.

use egui::{Align2, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};

use super::{area, draw};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// A few changed lines: old and new in one column, or side by side.
pub(super) fn diff_layout(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 110.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    // Unified.
    let u = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(u, t.radius, t.field);
    let rows = [
        (' ', 0.7),
        ('-', 0.55),
        ('-', 0.4),
        ('+', 0.6),
        ('+', 0.45),
        (' ', 0.5),
    ];
    for (i, (kind, w)) in rows.iter().enumerate() {
        diff_line(&p, t, u, i, *kind, *w);
    }
    draw::choice(
        &p,
        t,
        u,
        crate::i18n::text("interaction-preview-unified"),
        !s.diff_split,
    );
    // Side by side: the old lines left, the new right.
    let sx = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(sx, t.radius, t.field);
    let half = (sx.width() - 1.0) / 2.0;
    let left = Rect::from_min_size(sx.min, vec2(half, h));
    let right = Rect::from_min_size(pos2(sx.left() + half + 1.0, sx.top()), vec2(half, h));
    p.line_segment(
        [
            pos2(left.right() + 0.5, sx.top()),
            pos2(left.right() + 0.5, sx.bottom()),
        ],
        Stroke::new(1.0, t.separator),
    );
    for (i, (l, r, w)) in [
        (' ', ' ', 0.7),
        ('-', '+', 0.6),
        ('-', '+', 0.45),
        (' ', ' ', 0.5),
    ]
    .iter()
    .enumerate()
    {
        diff_line(&p, t, left, i, *l, *w);
        diff_line(&p, t, right, i, *r, *w * 0.9);
    }
    draw::choice(
        &p,
        t,
        sx,
        crate::i18n::text("interaction-preview-side"),
        s.diff_split,
    );
}

/// Line `i` of a diff drawn in `r`: its number, sign, background and a bar for its text.
fn diff_line(p: &Painter, t: &Tokens, r: Rect, i: usize, kind: char, w: f32) {
    let h = 16.0;
    let y = r.top() + 8.0 + h * (i as f32 + 0.5);
    let band = Rect::from_min_size(pos2(r.left(), y - h / 2.0), vec2(r.width(), h));
    let (bg, fg) = match kind {
        '-' => (Some(t.diff_del_bg), t.danger),
        '+' => (Some(t.diff_add_bg), t.success),
        '@' => (Some(t.accent.gamma_multiply(0.12)), t.accent_fg),
        _ => (None, t.text_dim),
    };
    if let Some(bg) = bg {
        p.rect_filled(band, 0.0, bg);
    }
    p.text(
        pos2(r.left() + 6.0, y),
        Align2::LEFT_CENTER,
        format!("{}", 40 + i),
        theme::mono(9.5),
        t.text_faint,
    );
    if kind == '@' {
        p.text(
            pos2(r.left() + 26.0, y),
            Align2::LEFT_CENTER,
            "@@ -52,6 +52,7 @@",
            theme::mono(9.5),
            fg,
        );
        return;
    }
    if kind == '-' || kind == '+' {
        p.text(
            pos2(r.left() + 26.0, y),
            Align2::LEFT_CENTER,
            kind.to_string(),
            theme::mono(10.0),
            fg,
        );
    }
    let x = r.left() + 36.0;
    draw::bar(
        p,
        x,
        y,
        (r.right() - x - 6.0) * w,
        4.0,
        if kind == ' ' {
            t.text_faint
        } else {
            fg.gamma_multiply(0.8)
        },
    );
}

/// The lines around the changes, with what lies between folded; or the whole file.
pub(super) fn diff_context(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 150.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let around = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(around, t.radius, t.field);
    let rows = [('@', 0.0), (' ', 0.6), ('-', 0.5), ('+', 0.55), (' ', 0.4)];
    for (i, (k, w)) in rows.iter().enumerate() {
        diff_line(&p, t, around, i, *k, *w);
    }
    let fold_y = around.top() + 8.0 + 16.0 * 5.0 + 10.0;
    p.text(
        pos2(around.center().x, fold_y),
        Align2::CENTER_CENTER,
        crate::i18n::text("interaction-preview-unchanged"),
        FontId::proportional(10.5),
        t.text_faint,
    );
    draw::choice(
        &p,
        t,
        around,
        crate::i18n::text("interaction-preview-around"),
        !s.diff_whole_file,
    );
    let whole = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(whole, t.radius, t.field);
    let rows = [
        (' ', 0.5),
        (' ', 0.7),
        (' ', 0.35),
        (' ', 0.6),
        ('-', 0.5),
        ('+', 0.55),
        (' ', 0.4),
        (' ', 0.65),
    ];
    for (i, (k, w)) in rows.iter().enumerate() {
        diff_line(&p, t, whole, i, *k, *w);
    }
    draw::choice(
        &p,
        t,
        whole,
        crate::i18n::text("interaction-preview-whole"),
        s.diff_whole_file,
    );
}
