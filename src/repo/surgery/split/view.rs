//! The split editor in the big area: the new commits and their messages on the left, the old
//! commit's files on the right with a chip per change saying which commit it goes to (click a
//! file, a hunk header or a line to give it to the selected commit; 1–9 select a commit), then
//! each new commit's patch to check before splitting.

mod groups;
use groups::*;
mod changes;
use changes::*;

use egui::{Color32, Key, RichText, Sense, Stroke, Ui};

use super::preview::Look;
use super::{MAX_GROUPS, Mode, Row, SplitEditor};
use crate::icons;
use crate::repo::diff::Kind;
use crate::repo::surgery::moving::Dest;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 20.0;

/// What a click in the list asks for.
enum Click {
    Fold(usize),
    File(usize),
    Hunk(usize, usize),
    Line(usize, usize),
}

/// The colour of new commit `g`.
fn color(t: &Tokens, g: usize) -> Color32 {
    t.lane(g)
}

/// Black or white, whichever reads better on `fill`.
fn ink_on(fill: Color32) -> Color32 {
    let [r, g, b, _] = fill.to_array();
    let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if luma > 150.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// A chip with the number of new commit `g`, filled with its colour.
fn chip(ui: &mut Ui, t: &Tokens, g: usize) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 15.0), Sense::hover());
    let fill = color(t, g);
    ui.painter().rect_filled(rect, t.radius_sm, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        (g + 1).to_string(),
        theme::semibold(10.5),
        ink_on(fill),
    );
}

impl RepoTab {
    pub(in crate::repo) fn split_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let keys_free =
            self.dialog.is_none() && !self.keys_blocked && !ui.ctx().egui_wants_keyboard_input();
        let Some(editor) = self.split.as_deref_mut() else {
            return;
        };
        if keys_free && editor.mode == Mode::Split {
            pick_with_keys(ui, editor);
        }
        header(ui, &t, editor, acts);
        footer(ui, &t, editor, idle, acts);
        egui::Panel::left("split_groups")
            .resizable(true)
            .default_size(300.0)
            .size_range(220.0..=520.0)
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .inner_margin(egui::Margin::same(10))
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                if matches!(editor.mode, Mode::Move(_)) {
                    destinations(ui, &t, editor);
                } else {
                    groups(ui, &t, editor);
                }
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.dock))
            .show(ui, |ui| {
                if editor.checked.is_some() {
                    patches(ui, &t, editor);
                } else {
                    changes(ui, &t, editor);
                }
            });
    }
}

/// 1–9 select the commit clicks go to.
fn pick_with_keys(ui: &Ui, editor: &mut SplitEditor) {
    const DIGITS: [Key; 9] = [
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ];
    for (g, key) in DIGITS.into_iter().enumerate() {
        if g < editor.groups.len() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
            editor.active = g;
        }
    }
}

fn header(ui: &mut Ui, t: &Tokens, editor: &SplitEditor, acts: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0, t.separator))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let icon = match editor.mode {
                    Mode::Split => "rows-2",
                    Mode::Move(_) => "file-diff",
                };
                icons::inline(ui, icon, 16.0, t.icon);
                let verb = match editor.mode {
                    Mode::Split => crate::i18n::text("repo-workflows-surgery-header-32afaa78"),
                    Mode::Move(_) => crate::i18n::text("repo-workflows-surgery-header-08e79585"),
                };
                ui.label(
                    RichText::new(format!(
                        "{verb} {} · {}",
                        short(&editor.hash),
                        editor.summary
                    ))
                    .font(theme::medium(13.0)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(
                        ui,
                        "x",
                        crate::i18n::text("repo-workflows-surgery-header-862c2324"),
                    )
                    .clicked()
                    {
                        acts.push(Action::CloseSplit);
                    }
                });
            });
        });
}

fn footer(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor, idle: bool, acts: &mut Vec<Action>) {
    let problem = editor.problem();
    egui::Panel::bottom("split_footer")
        .frame(
            egui::Frame::new()
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            // The buttons first, from the right: the note takes what is left, cut short, all
            // of it in its tooltip.
            let moving = matches!(editor.mode, Mode::Move(_));
            let (check, run) = if moving {
                (
                    crate::i18n::text("repo-workflows-surgery-footer-b5292604"),
                    crate::i18n::text("repo-workflows-surgery-footer-42680e0d"),
                )
            } else {
                (
                    crate::i18n::text("repo-workflows-surgery-footer-11188750"),
                    crate::i18n::text("repo-workflows-surgery-footer-df9d2856"),
                )
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if editor.checked.is_some() {
                    if widgets::primary_button(ui, run, idle && !editor.busy, 120.0).clicked() {
                        acts.push(Action::RunSplit);
                    }
                    if ui
                        .add_enabled(
                            !editor.busy,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-surgery-footer-76900f1b",
                            )),
                        )
                        .clicked()
                    {
                        editor.checked = None;
                    }
                } else if widgets::primary_button(
                    ui,
                    check,
                    problem.is_none() && !editor.busy,
                    120.0,
                )
                .clicked()
                {
                    acts.push(Action::CheckSplit);
                }
                if ui
                    .button(crate::i18n::text("repo-workflows-surgery-footer-19766ed6"))
                    .clicked()
                {
                    acts.push(Action::CloseSplit);
                }
                if editor.busy {
                    ui.add(crate::spinner::Spinner::new());
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let (text, color) = note(t, editor, problem);
                    ui.add(egui::Label::new(RichText::new(&text).color(color)).truncate())
                        .on_hover_text(text);
                });
            });
        });
}

/// What the footer says, and in which colour.
fn note(t: &Tokens, editor: &SplitEditor, problem: Option<(String, bool)>) -> (String, Color32) {
    match (&editor.error, problem, &editor.checked) {
        (Some(e), _, _) if matches!(editor.mode, Mode::Move(_)) => (
            crate::i18n::message(
                "repo-workflows-surgery-note-a227d3be",
                &[("e", (e).clone().into())],
            ),
            t.danger,
        ),
        (Some(e), _, _) => (
            crate::i18n::message(
                "repo-workflows-surgery-note-e5a23195",
                &[("e", (e).clone().into())],
            ),
            t.danger,
        ),
        (None, Some((p, hint)), _) => (p, if hint { t.text_dim } else { t.danger }),
        (None, None, Some(_)) if matches!(editor.mode, Mode::Move(_)) => (
            crate::i18n::text("repo-workflows-surgery-note-85eaa81d").to_owned(),
            t.text_dim,
        ),
        (None, None, Some(c)) => (
            crate::i18n::message(
                "repo-workflows-surgery-note-a3a06218",
                &[(
                    "c-trees-len-saturating-sub-1-1",
                    (c.trees.len().saturating_sub(1)).into(),
                )],
            ),
            t.text_dim,
        ),
        (None, None, None) if matches!(editor.mode, Mode::Move(_)) => (
            crate::i18n::text("repo-workflows-surgery-note-4656ae85").to_owned(),
            t.text_dim,
        ),
        (None, None, None) => (
            crate::i18n::text("repo-workflows-surgery-note-c96c4852").to_owned(),
            t.text_dim,
        ),
    }
}

/// Each new commit's patch, as checked, a screenful at a time.
fn patches(ui: &mut Ui, t: &Tokens, editor: &SplitEditor) {
    let Some(checked) = &editor.checked else {
        return;
    };
    let lines = &checked.preview.lines;
    egui::ScrollArea::both()
        .id_salt(("split_patches", editor.id))
        .auto_shrink(false)
        .show_rows(ui, ROW_H, lines.len(), |ui, range| {
            for (look, text) in lines.get(range).unwrap_or_default() {
                preview_line(ui, t, editor, *look, text);
            }
        });
}

fn preview_line(ui: &mut Ui, t: &Tokens, editor: &SplitEditor, look: Look, text: &str) {
    let width = ui.available_width().max(400.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW_H), Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    let mono = |color: Color32| RichText::new(text).font(theme::mono(12.0)).color(color);
    let label = match look {
        Look::Commit(g, files) => {
            ui.painter().rect_filled(rect, 0.0, t.chrome);
            ui.add_space(8.0);
            chip(ui, t, g);
            let subject = editor
                .groups
                .get(g)
                .map(|g| g.subject.as_str())
                .unwrap_or_default();
            ui.label(RichText::new(subject).font(theme::medium(13.0)));
            if files == 0 {
                RichText::new(crate::i18n::message(
                    "repo-workflows-surgery-preview-line-4a4c8a43",
                    &[("short-editor-hash-1", (short(&editor.hash)).into())],
                ))
                .color(t.text_dim)
            } else {
                RichText::new(crate::i18n::message(
                    "repo-workflows-surgery-preview-files",
                    &[("count", (files).into()), ("text", (text).into())],
                ))
                .color(t.text_dim)
            }
        }
        Look::Header => mono(t.accent),
        Look::Meta => {
            if text == "(binary contents)" {
                RichText::new(crate::i18n::message(
                    "repo-workflows-surgery-binary-preview",
                    &[],
                ))
                .font(theme::mono(11.5))
                .color(t.text_faint)
            } else {
                mono(t.text_faint)
            }
        }
        Look::Context => mono(t.text_dim),
        Look::Add | Look::Del => {
            let fill = if look == Look::Add {
                t.diff_add_bg
            } else {
                t.diff_del_bg
            };
            ui.painter().rect_filled(rect, 0.0, fill);
            mono(t.text)
        }
    };
    ui.add(egui::Label::new(label).selectable(false).extend());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_numbers_read_on_light_and_dark_colours() {
        assert_eq!(ink_on(Color32::from_rgb(250, 200, 60)), Color32::BLACK);
        assert_eq!(ink_on(Color32::from_rgb(40, 60, 160)), Color32::WHITE);
    }
}
