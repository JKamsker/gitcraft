//! Tree and flat changed-file previews.

use egui::{Align2, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};

use super::{area, draw};
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// The changed files in their folders, or in one list.
pub(super) fn file_list(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 12.0;
    let each = (width - gap) / 2.0;
    let h = 132.0;
    let rect = area(ui, width, h + 26.0);
    let p = ui.painter_at(rect);
    let row_h = 20.0;
    let tree = Rect::from_min_size(rect.min, vec2(each, h));
    p.rect_filled(tree, t.radius, t.card);
    let entries: [(usize, &str, Option<char>); 6] = [
        (0, "src", None),
        (1, "theme", None),
        (2, "catalog.rs", Some('A')),
        (2, "mod.rs", Some('M')),
        (1, "settings.rs", Some('M')),
        (0, "README.md", Some('M')),
    ];
    for (i, (depth, name, status)) in entries.iter().enumerate() {
        let y = tree.top() + 8.0 + row_h * (i as f32 + 0.5);
        let x = tree.left() + 10.0 + *depth as f32 * 14.0;
        file_entry(&p, t, tree, x, y, name, *status);
    }
    draw::choice(
        &p,
        t,
        tree,
        crate::i18n::text("interaction-preview-folders"),
        s.files_as_tree,
    );
    let list = Rect::from_min_size(pos2(rect.left() + each + gap, rect.top()), vec2(each, h));
    p.rect_filled(list, t.radius, t.card);
    let files: [(&str, &str, char); 4] = [
        ("catalog.rs", "src/theme", 'A'),
        ("mod.rs", "src/theme", 'M'),
        ("settings.rs", "src", 'M'),
        ("README.md", "", 'M'),
    ];
    for (i, (name, folder, status)) in files.iter().enumerate() {
        let y = list.top() + 8.0 + row_h * (i as f32 + 0.5);
        let r = file_entry(&p, t, list, list.left() + 10.0, y, name, Some(*status));
        if !folder.is_empty() {
            draw::text(
                &p,
                pos2(r.right() + 6.0, y),
                Align2::LEFT_CENTER,
                folder,
                FontId::proportional(10.5),
                t.text_faint,
                list.right() - r.right() - 30.0,
            );
        }
    }
    draw::choice(
        &p,
        t,
        list,
        crate::i18n::text("interaction-preview-list"),
        !s.files_as_tree,
    );
}

/// A file or folder of a changed-files list; returns the rect of its name.
fn file_entry(
    p: &Painter,
    t: &Tokens,
    area: Rect,
    x: f32,
    y: f32,
    name: &str,
    status: Option<char>,
) -> Rect {
    let font = FontId::proportional(11.5);
    match status {
        None => {
            // A folder: an open chevron.
            p.add(egui::Shape::line(
                vec![
                    pos2(x - 3.0, y - 2.0),
                    pos2(x, y + 1.5),
                    pos2(x + 3.0, y - 2.0),
                ],
                Stroke::new(1.3, t.text_faint),
            ));
            draw::text(
                p,
                pos2(x + 8.0, y),
                Align2::LEFT_CENTER,
                name,
                font,
                t.text_dim,
                area.right() - x - 14.0,
            )
        }
        Some(code) => {
            let color = crate::repo::widgets::status_color(t, code);
            p.text(
                pos2(area.right() - 12.0, y),
                Align2::CENTER_CENTER,
                code.to_string(),
                theme::semibold(10.5),
                color,
            );
            draw::text(
                p,
                pos2(x, y),
                Align2::LEFT_CENTER,
                name,
                font,
                t.text,
                area.right() - x - 30.0,
            )
        }
    }
}
