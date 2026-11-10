//! Terminal font and shell preview.

use egui::{Align2, Color32, Rect, Stroke, Ui, pos2, vec2};

use super::area;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// The terminal with the default shell's prompt, at the font size set, in the theme's
/// colours.
pub(super) fn terminal(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let size = s.terminal_font.clamp(9.0, 20.0);
    let line_h = size * 1.45;
    let shell = s.shell.to_lowercase();
    // The prompt as the shell set writes it by default (the system's: as on Linux or macOS).
    let prompt: Vec<(&str, Color32)> = if shell.contains("pwsh") || shell.contains("powershell") {
        vec![("PS C:\\code\\gitcraft> ", t.text)]
    } else if shell.contains("cmd") {
        vec![("C:\\code\\gitcraft>", t.text)]
    } else if shell.contains("fish") {
        vec![
            ("~/gitcraft", t.ansi[2]),
            (" (main)", t.ansi[5]),
            ("> ", t.text),
        ]
    } else {
        vec![
            ("me@laptop", t.ansi[2]),
            (":", t.text),
            ("~/gitcraft", t.ansi[4]),
            (" (main)", t.ansi[5]),
            ("$ ", t.text),
        ]
    };
    let lines: Vec<Vec<(&str, Color32)>> = vec![
        {
            let mut l = prompt.clone();
            l.push(("git status --short", t.text));
            l
        },
        vec![(" M ", t.ansi[1]), ("src/theme/mod.rs", t.text)],
        vec![("A  ", t.ansi[2]), ("src/theme/catalog.rs", t.text)],
        vec![("?? ", t.ansi[1]), ("themes/my-theme.json", t.text)],
        {
            let mut l = prompt.clone();
            l.push(("git log --oneline -2", t.text));
            l
        },
        vec![
            ("3f9a2c1 ", t.ansi[3]),
            ("(HEAD -> main) ", t.ansi[6]),
            (
                crate::i18n::text("interaction-preview-terminal-theme"),
                t.text,
            ),
        ],
        vec![
            ("b81e07d ", t.ansi[3]),
            (
                crate::i18n::text("interaction-preview-terminal-merge"),
                t.text,
            ),
        ],
        {
            let mut l = prompt;
            l.push(("▌", t.text_dim));
            l
        },
    ];
    let h = 16.0 + lines.len() as f32 * line_h + 34.0;
    let rect = area(ui, width, h);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, t.radius, t.field);
    p.rect_stroke(
        rect,
        t.radius,
        Stroke::new(1.0, t.field_border),
        egui::StrokeKind::Inside,
    );
    let font = theme::mono(size);
    for (i, line) in lines.iter().enumerate() {
        let mut x = rect.left() + 10.0;
        let y = rect.top() + 8.0 + line_h * (i as f32 + 0.5);
        for (text, color) in line {
            let r = p.text(pos2(x, y), Align2::LEFT_CENTER, *text, font.clone(), *color);
            x = r.right();
        }
    }
    // The theme's 16 colours.
    let y = rect.bottom() - 22.0;
    let sw = ((rect.width() - 20.0) / 16.0).min(22.0);
    for (i, c) in t.ansi.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + 10.0 + i as f32 * sw, y),
            vec2(sw - 3.0, 12.0),
        );
        p.rect_filled(r, 2.0, *c);
    }
}
