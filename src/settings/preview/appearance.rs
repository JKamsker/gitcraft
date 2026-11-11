//! Code color and avatar previews.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Ui, pos2, vec2};

use super::{area, draw};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

use super::graph::{initials, name_hash};
/// Code in diffs and blame: a few lines of Rust as a diff colours them.
pub(super) fn code(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let lines = [
        "/// Paints the app with `t` from now on.",
        "pub fn apply(ctx: &Context, id: &ThemeId) {",
        "    let t = resolve(id).tokens();",
        "    let t = resolve(id).tokens(&tweaks);",
        "    ctx.set_visuals(visuals(&t)); // 42 colours",
        "}",
    ];
    let kinds = [' ', ' ', '-', '+', ' ', ' '];
    let spans = crate::repo::highlight::highlight("preview.rs", lines.iter().copied(), t.dark);
    let line_h = 18.0;
    let rect = area(ui, width, lines.len() as f32 * line_h + 12.0);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, t.radius, t.field);
    let font = theme::mono(11.5);
    for (i, line) in lines.iter().enumerate() {
        let y = rect.top() + 6.0 + line_h * (i as f32 + 0.5);
        let band = Rect::from_min_size(
            pos2(rect.left(), y - line_h / 2.0),
            vec2(rect.width(), line_h),
        );
        let (bg, sign) = match kinds[i] {
            '-' => (Some(t.diff_del_bg), "-"),
            '+' => (Some(t.diff_add_bg), "+"),
            _ => (None, " "),
        };
        if let Some(bg) = bg {
            p.rect_filled(band, 0.0, bg);
        }
        p.text(
            pos2(rect.left() + 8.0, y),
            Align2::LEFT_CENTER,
            format!("{}", i + 12),
            theme::mono(10.5),
            t.text_faint,
        );
        p.text(
            pos2(rect.left() + 32.0, y),
            Align2::LEFT_CENTER,
            sign,
            font.clone(),
            t.text_faint,
        );
        let mut x = rect.left() + 44.0;
        let clip = p.with_clip_rect(band.shrink2(vec2(4.0, 0.0)));
        match spans.as_ref().and_then(|s| s.get(i)) {
            Some(parts) => {
                for (color, text) in parts {
                    let r = clip.text(pos2(x, y), Align2::LEFT_CENTER, text, font.clone(), *color);
                    x = r.right();
                }
            }
            None => {
                clip.text(pos2(x, y), Align2::LEFT_CENTER, *line, font.clone(), t.text);
            }
        }
    }
    ui.add_space(6.0);
    let note = if !s.vscode_colors {
        crate::i18n::text("interaction-preview-code-own").to_owned()
    } else {
        match crate::vscode_theme::current() {
            Some(found) if found.dark == t.dark => crate::i18n::message(
                "interaction-preview-code-editor",
                &[("theme", found.name.as_str().into())],
            ),
            Some(found) => crate::i18n::message(
                "interaction-preview-code-mismatch",
                &[
                    ("theme", found.name.as_str().into()),
                    ("mode", if found.dark { "dark" } else { "light" }.into()),
                ],
            ),
            None => crate::i18n::text("interaction-preview-no-vscode").to_owned(),
        }
    };
    super::super::layout::note(ui, note);
}

/// Authors with their pictures, or with their initials.
pub(super) fn avatars(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 96.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let people = ["Ada Lovelace", "Grace Hopper", "Linus Torvalds"];
    for (i, (name, pictures)) in [
        (crate::i18n::text("interaction-preview-initials"), false),
        (crate::i18n::text("interaction-preview-pictures"), true),
    ]
    .into_iter()
    .enumerate()
    {
        let r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (each + gap), rect.top()),
            vec2(each, h),
        );
        p.rect_filled(r, t.radius, t.dock);
        for (k, who) in people.iter().enumerate() {
            let y = r.top() + 18.0 + k as f32 * 28.0;
            let c = pos2(r.left() + 20.0, y);
            if pictures {
                face(&p, t, c, 10.0, k);
            } else {
                p.circle_filled(c, 10.0, t.lane(name_hash(who)));
                p.text(
                    c,
                    Align2::CENTER_CENTER,
                    initials(who),
                    theme::semibold(8.0),
                    Color32::WHITE,
                );
            }
            draw::text(
                &p,
                pos2(c.x + 16.0, y),
                Align2::LEFT_CENTER,
                who,
                FontId::proportional(11.5),
                t.text_dim,
                r.right() - c.x - 22.0,
            );
        }
        draw::choice(&p, t, r, name, s.avatars == pictures);
    }
}

/// A made-up picture: a head and shoulders on a coloured ground.
fn face(p: &Painter, t: &Tokens, c: Pos2, radius: f32, k: usize) {
    let grounds = [t.lane(5), t.lane(3), t.lane(4)];
    p.circle_filled(
        c,
        radius,
        grounds[k % 3].lerp_to_gamma(Color32::WHITE, 0.35),
    );
    let skin = [
        Color32::from_rgb(241, 194, 160),
        Color32::from_rgb(198, 134, 96),
        Color32::from_rgb(141, 85, 54),
    ][k % 3];
    let clip = p.with_clip_rect(Rect::from_center_size(c, vec2(radius * 2.0, radius * 2.0)));
    clip.circle_filled(
        c + vec2(0.0, radius * 1.05),
        radius * 0.75,
        grounds[k % 3].lerp_to_gamma(Color32::BLACK, 0.3),
    );
    clip.circle_filled(c + vec2(0.0, -radius * 0.15), radius * 0.4, skin);
}
