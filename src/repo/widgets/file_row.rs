//! A changed file's row in a file list or tree.

use egui::{Align2, CornerRadius, Painter, Rect, Response, Sense, Ui, pos2, vec2};

use super::{ROW_H, status_color};
use crate::a11y::labels;
use crate::icons;
use crate::theme::{self, Tokens};

/// What a file row shows on its right edge.
#[derive(Clone, Copy)]
pub enum RowExtra<'a> {
    /// `+added −removed` (`None` for binary files).
    Stats(Option<u32>, Option<u32>),
    /// Icon buttons shown while the row is hovered: `(icon, tooltip)`.
    Buttons(&'a [(&'a str, &'a str)]),
    /// [`RowExtra::Stats`] or [`RowExtra::Buttons`] with a warning badge left of them.
    Badged(&'a RowExtra<'a>, &'a str),
}

pub struct FileRowResponse {
    pub row: Response,
    /// Index of the clicked hover button.
    pub button: Option<usize>,
    /// Where the badge of [`RowExtra::Badged`] is, for its own hover text.
    pub badge: Option<Rect>,
}

/// How wide each hover button is, with its gap: room kept for them beside the owners.
const BUTTON_ROOM: f32 = 22.0;

/// Who owns a file (CODEOWNERS), as its row shows it.
pub struct OwnerTag {
    /// Short, on the row: `payments +1`.
    pub label: String,
    /// Every owner, for the row's tooltip.
    pub full: String,
}

/// One file: status letter, file name, its folder dimmed, and `extra` on the right.
/// In a tree (`indent` is how far in) the folder is above the row instead.
pub fn file_row(
    ui: &mut Ui,
    code: char,
    path: &str,
    old_path: Option<&str>,
    indent: Option<f32>,
    selected: bool,
    extra: RowExtra<'_>,
) -> FileRowResponse {
    file_row_owned(ui, code, path, old_path, indent, selected, extra, None)
}

/// [`file_row`], with the file's owners left of `extra`.
#[allow(clippy::too_many_arguments)]
pub fn file_row_owned(
    ui: &mut Ui,
    code: char,
    path: &str,
    old_path: Option<&str>,
    indent: Option<f32>,
    selected: bool,
    extra: RowExtra<'_>,
    owners: Option<&OwnerTag>,
) -> FileRowResponse {
    let shift = indent.unwrap_or(0.0);
    let t = Tokens::get(ui.ctx());
    // By its path: a status reload shifts the list, and an open menu (Ignore…) or a pressed
    // Stage must stay this file's. A list that can show a path another one shows too (staged
    // and unstaged) is drawn under an id of its own.
    let (rect, row) = super::keyed_row(ui, ROW_H, ("file-row", path));
    super::a11y_selected(&row, || labels::file(path, code, old_path), selected);
    let hovered = ui.rect_contains_pointer(rect);
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, t.row_selected);
    } else if hovered {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;

    // Status letter in a small rounded square; none for a file that isn't changed (`' '`).
    if code != ' ' {
        let color = status_color(&t, code);
        let badge = Rect::from_center_size(pos2(rect.left() + 18.0 + shift, y), vec2(16.0, 16.0));
        painter.rect_filled(
            badge,
            CornerRadius::same(3),
            t.text_tint(
                color,
                0.22,
                if selected {
                    t.row_selected
                } else if hovered {
                    t.hover
                } else {
                    t.card
                },
            ),
        );
        let letter = if code == '?' { 'A' } else { code };
        painter.text(
            badge.center(),
            Align2::CENTER_CENTER,
            letter.to_string(),
            theme::semibold(10.5),
            color,
        );
    }

    // Right edge first, so the name is clipped before it.
    let mut right = rect.right() - 8.0;
    let mut button = None;
    let mut badge = None;
    let (extra, label) = match &extra {
        RowExtra::Badged(inner, label) => (*inner, Some(*label)),
        other => (other, None),
    };
    match extra {
        RowExtra::Stats(added, removed) => {
            right = stats(&painter, &t, right, y, *added, *removed);
        }
        RowExtra::Buttons(buttons) => {
            // Also while the keyboard is on the row or one of them, so Tab reaches them.
            let keyboard = crate::a11y::keyboard_on(&row)
                || (0..buttons.len()).any(|i| ui.memory(|m| m.has_focus(button_id(&row, i))));
            if hovered || selected || keyboard {
                (right, button) = hover_buttons(ui, &painter, &t, &row, right, buttons);
            } else if label.is_some() {
                // The badge stays where it is when the buttons come up on hover.
                right -= BUTTON_W * buttons.len() as f32;
            } else if owners.is_some() {
                // The owners stay put when the buttons come with the pointer.
                right -= BUTTON_ROOM * buttons.len() as f32;
            }
        }
        RowExtra::Badged(..) => {}
    }
    if let Some(label) = label {
        // Room left for the name; with too little, the badge shrinks to its icon.
        let room = right - (rect.left() + 32.0 + shift);
        let at = warning_badge(ui, &painter, &t, (right, y), label, room < 180.0);
        right = at.left() - 6.0;
        badge = Some(at);
    }

    if let Some(owners) = owners {
        let room = ((right - rect.left()) * 0.4).max(0.0);
        let clip = Rect::from_min_max(pos2(right - room, rect.top()), pos2(right, rect.bottom()));
        let r = painter.with_clip_rect(clip).text(
            pos2(right - 4.0, y),
            Align2::RIGHT_CENTER,
            &owners.label,
            egui::FontId::proportional(11.0),
            t.accent,
        );
        right = r.left().max(clip.left()) - 8.0;
    }

    let (dir, name) = split_dir(path);
    let text_clip = Rect::from_min_max(
        pos2(rect.left() + 32.0 + shift, rect.top()),
        pos2(right, rect.bottom()),
    );
    let tp = painter.with_clip_rect(text_clip);
    let name_rect = tp.text(
        pos2(text_clip.left(), y),
        Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(12.5),
        t.text,
    );
    let mut faint = String::new();
    if let Some(old) = old_path {
        faint.push_str("← ");
        faint.push_str(old);
        faint.push_str("  ");
    }
    if indent.is_none() {
        faint.push_str(dir);
    }
    if !faint.is_empty() {
        tp.text(
            pos2(name_rect.right() + 8.0, y),
            Align2::LEFT_CENTER,
            faint,
            egui::FontId::proportional(11.5),
            t.text_faint,
        );
    }
    // The full path only adds something when the row shows it cut short.
    let shown = match old_path {
        Some(old) => Some(format!("{old} → {path}")),
        None if !dir.is_empty() => Some(path.to_owned()),
        None => None,
    };
    let tip = match (shown, owners) {
        (Some(p), Some(o)) => Some(format!("{p}\nOwners: {}", o.full)),
        (None, Some(o)) => Some(format!("Owners: {}", o.full)),
        (shown, None) => shown,
    };
    let row = match tip {
        Some(tip) => row.on_hover_text(tip),
        None => row,
    };
    FileRowResponse { row, button, badge }
}

/// Width a hover button takes, with its gap.
const BUTTON_W: f32 = 22.0;

/// A small warning pill with `label` (only its icon when `compact`), right-aligned at
/// `right`; returns where it is.
fn warning_badge(
    ui: &Ui,
    painter: &Painter,
    t: &Tokens,
    (right, y): (f32, f32),
    label: &str,
    compact: bool,
) -> Rect {
    let color = badge_text(t);
    if compact {
        let rect = Rect::from_center_size(pos2(right - 9.0, y), vec2(18.0, 18.0));
        painter.rect_filled(rect, CornerRadius::same(4), t.warning.gamma_multiply(0.18));
        icons::paint(ui, rect, "circle-alert", 12.0, color);
        return rect;
    }
    let galley = painter.layout_no_wrap(label.to_owned(), theme::medium(10.0), color);
    let size = galley.size() + vec2(10.0, 3.0);
    let rect = Rect::from_min_size(pos2(right - size.x, y - size.y / 2.0), size);
    painter.rect_filled(rect, CornerRadius::same(4), t.warning.gamma_multiply(0.18));
    painter.galley(rect.min + vec2(5.0, 1.5), galley, color);
    rect
}

/// The warning colour, darkened on light themes so small text on its tint stays readable.
pub fn badge_text(t: &Tokens) -> egui::Color32 {
    if t.dark() {
        t.warning
    } else {
        let [r, g, b, _] = t.warning.to_array();
        let dim = |c: u8| (f32::from(c) * 0.72) as u8;
        egui::Color32::from_rgb(dim(r), dim(g), dim(b))
    }
}

/// `+added −removed` (or `bin`) right-aligned at `right`; returns where the name must end.
fn stats(
    painter: &Painter,
    t: &Tokens,
    right: f32,
    y: f32,
    added: Option<u32>,
    removed: Option<u32>,
) -> f32 {
    let small = theme::mono(11.0);
    match (added, removed) {
        (Some(a), Some(d)) => {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                format!("−{d}"),
                small.clone(),
                t.danger,
            );
            let r = painter.text(
                pos2(r.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                format!("+{a}"),
                small,
                t.success,
            );
            r.left() - 8.0
        }
        _ => {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                "bin",
                small,
                t.text_faint,
            );
            r.left() - 8.0
        }
    }
}

/// The row's icon buttons, right-aligned at `right`; returns where the name must end and the
/// clicked button.
fn hover_buttons(
    ui: &Ui,
    painter: &Painter,
    t: &Tokens,
    row: &Response,
    mut right: f32,
    buttons: &[(&str, &str)],
) -> (f32, Option<usize>) {
    let y = row.rect.center().y;
    let mut clicked = None;
    for (i, (icon, tooltip)) in buttons.iter().enumerate().rev() {
        let r = Rect::from_center_size(pos2(right - 10.0, y), vec2(20.0, 20.0));
        let resp = ui
            .interact(r, button_id(row, i), Sense::click())
            .on_hover_text(*tooltip);
        super::a11y(&resp, egui::WidgetType::Button, *tooltip);
        if resp.hovered() {
            painter.rect_filled(r, t.radius_sm, t.pressed);
        }
        icons::paint(
            ui,
            r,
            icon,
            14.0,
            if resp.hovered() { t.text } else { t.icon },
        );
        if resp.clicked() {
            clicked = Some(i);
        }
        right = r.left() - 2.0;
    }
    (right, clicked)
}

/// The `i`th icon button of `row`.
fn button_id(row: &Response, i: usize) -> egui::Id {
    row.id.with(("btn", i))
}

/// `(folder, name)` of a path: `src/repo` and `mod.rs`. A slash at the end (git shows a nested
/// repository as `sub/`) stays with the name.
pub(super) fn split_dir(path: &str) -> (&str, &str) {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    match trimmed.rsplit_once('/') {
        Some((dir, _)) => (dir, &path[dir.len() + 1..]),
        None => ("", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painted_file_badges_meet_aa_on_normal_hovered_and_selected_rows() {
        let ctx = theme::test_paint::context();
        for builtin in theme::catalog::BUILTIN {
            let t = builtin.palette().tokens();
            theme::set_tokens(&ctx, t);
            for state in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events: vec![egui::Event::PointerMoved(if state == 1 {
                            pos2(20.0, 20.0)
                        } else {
                            pos2(700.0, 700.0)
                        })],
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().fill(t.card))
                            .show(ctx, |ui| {
                                for code in ['M', 'A', 'D', 'R', 'C', 'U', '?'] {
                                    file_row(
                                        ui,
                                        code,
                                        &format!("{code}.rs"),
                                        None,
                                        None,
                                        state == 2,
                                        RowExtra::Stats(None, None),
                                    );
                                }
                            });
                    },
                );
                output.textures_delta.clear();
                for label in ["M", "A", "D", "R", "C", "U"] {
                    theme::test_paint::assert_text_aa(
                        &output.shapes,
                        t.card,
                        label,
                        &format!("{} state {state}", builtin.name),
                    );
                }
            }
        }
    }

    #[test]
    fn a_path_splits_into_folder_and_name() {
        assert_eq!(split_dir("src/repo/mod.rs"), ("src/repo", "mod.rs"));
        assert_eq!(split_dir("Cargo.toml"), ("", "Cargo.toml"));
        assert_eq!(split_dir("dös/ñame.rs"), ("dös", "ñame.rs"));
    }

    #[test]
    fn a_nested_repository_keeps_its_name() {
        // `git status` shows a repository inside the working tree as `sub/`.
        assert_eq!(split_dir("sub/"), ("", "sub/"));
        assert_eq!(split_dir("lib/sub/"), ("lib", "sub/"));
    }
}
