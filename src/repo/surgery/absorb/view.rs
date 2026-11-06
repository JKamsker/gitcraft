//! The absorb editor in the big area: each run of staged changes with the commit it goes into
//! (the clear matches taken, the others suggested or left staged, each changeable), then the
//! fixups and what stays staged to check before absorbing.

use egui::{Color32, RichText, Sense, Stroke, Ui};

use super::{AbsorbEditor, Match};
use crate::icons;
use crate::repo::diff::Kind;
use crate::repo::surgery::split::Look;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 20.0;
/// Lines of a change shown under it.
const SHOWN: usize = 4;

impl RepoTab {
    pub(in crate::repo) fn absorb_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let Some(editor) = self.absorb.as_deref_mut() else {
            return;
        };
        header(ui, &t, acts);
        footer(ui, &t, editor, idle, acts);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.dock))
            .show(ui, |ui| {
                if editor.checked.is_some() {
                    preview(ui, &t, editor);
                } else {
                    hunks(ui, &t, editor);
                }
            });
    }
}

fn header(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0, t.separator))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::inline(ui, "git-merge", 16.0, t.icon);
                ui.label(
                    RichText::new("Distribute staged fixes into the commits they belong to")
                        .font(theme::medium(13.0)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(ui, "x", "Cancel (Esc)").clicked() {
                        acts.push(Action::CloseAbsorb);
                    }
                });
            });
        });
}

fn footer(ui: &mut Ui, t: &Tokens, editor: &mut AbsorbEditor, idle: bool, acts: &mut Vec<Action>) {
    let problem = editor.problem();
    egui::Panel::bottom("absorb_footer")
        .frame(
            egui::Frame::new()
                .fill(t.chrome)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let free = idle && !editor.busy;
                if let Some(checked) = &editor.checked {
                    let n = checked.fixups.len();
                    let label = format!("Absorb into {n} commit{}", if n == 1 { "" } else { "s" });
                    if widgets::primary_button(ui, &label, free, 160.0).clicked() {
                        acts.push(Action::RunAbsorb);
                    }
                    if ui.add_enabled(free, egui::Button::new("Back")).clicked() {
                        editor.checked = None;
                    }
                } else if widgets::primary_button(ui, "Check", problem.is_none() && free, 100.0)
                    .clicked()
                {
                    acts.push(Action::CheckAbsorb);
                }
                if ui.button("Cancel").clicked() {
                    acts.push(Action::CloseAbsorb);
                }
                if editor.git_absorb
                    && ui
                        .add_enabled(free, egui::Button::new("Run git-absorb instead"))
                        .on_hover_text(
                            "The installed git-absorb picks the commits itself and rebases its fixups in",
                        )
                        .clicked()
                {
                    acts.push(Action::RunGitAbsorb);
                }
                if editor.busy {
                    ui.add(crate::spinner::Spinner::new());
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let (text, color) = match (&editor.error, &problem, &editor.checked) {
                        (Some(e), _, _) => (format!("Can't absorb them this way: {e}"), t.danger),
                        (None, Some(p), _) => (p.clone(), t.text_dim),
                        (None, None, Some(c)) => (
                            format!(
                                "Checked. Absorbing rewrites the branch from the oldest of them{}; Ctrl+Z undoes it",
                                if c.rest.is_some() { ", the rest staged again" } else { "" }
                            ),
                            t.text_dim,
                        ),
                        (None, None, None) => (
                            "Check to see each commit's fixup and what stays staged".to_owned(),
                            t.text_dim,
                        ),
                    };
                    ui.add(egui::Label::new(RichText::new(&text).color(color)).truncate())
                        .on_hover_text(text);
                });
            });
        });
}

/// What a match says, in which colour.
fn verdict(
    t: &Tokens,
    editor: &AbsorbEditor,
    found: Match,
    touched: &[usize],
) -> (String, Color32) {
    let name = |i: usize| {
        editor
            .targets
            .get(i)
            .map(|c| short(&c.hash).to_owned())
            .unwrap_or_default()
    };
    match found {
        Match::Clear(_) => (
            "its lines were last changed by this commit".to_owned(),
            t.success,
        ),
        Match::Several(_) => (
            format!(
                "several commits changed these lines ({}{}): the newest of the branch's is suggested",
                touched
                    .iter()
                    .map(|&i| name(i))
                    .collect::<Vec<_>>()
                    .join(", "),
                if touched.len() == 1 {
                    " and older ones"
                } else {
                    ""
                }
            ),
            t.warning,
        ),
        Match::None => (
            "no commit of the branch changed these lines: it stays staged unless you pick one"
                .to_owned(),
            t.text_faint,
        ),
    }
}

/// Each run of staged changes with where it goes.
fn hunks(ui: &mut Ui, t: &Tokens, editor: &mut AbsorbEditor) {
    let mut chosen: Option<(usize, Option<usize>)> = None;
    egui::ScrollArea::vertical()
        .id_salt(("absorb_hunks", editor.id))
        .auto_shrink(false)
        .show(ui, |ui| {
            if editor.busy {
                ui.disable();
            }
            ui.add_space(6.0);
            for (h, hunk) in editor.hunks.iter().enumerate() {
                let Some(file) = editor.files.get(hunk.file) else {
                    continue;
                };
                let choice = editor.choice.get(h).copied().flatten();
                ui.push_id(("absorb_hunk", h), |ui| {
                    egui::Frame::new()
                        .fill(t.card)
                        .stroke(Stroke::new(1.0, t.separator))
                        .corner_radius(t.radius)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let first = hunk.lines.first().and_then(|&i| file.lines.get(i));
                                let at = first.and_then(|l| l.new.or(l.old)).unwrap_or(0);
                                ui.label(RichText::new(&file.path).font(theme::medium(12.5)));
                                ui.label(RichText::new(format!("line {at}")).color(t.text_faint));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let shown = match choice.and_then(|c| editor.targets.get(c))
                                        {
                                            Some(c) => format!("{} {}", short(&c.hash), c.summary),
                                            None => "Leave staged".to_owned(),
                                        };
                                        egui::ComboBox::from_id_salt("into")
                                            .width(280.0)
                                            .truncate()
                                            .selected_text(shown)
                                            .show_ui(ui, |ui| {
                                                if ui
                                                    .selectable_label(
                                                        choice.is_none(),
                                                        "Leave staged",
                                                    )
                                                    .clicked()
                                                {
                                                    chosen = Some((h, None));
                                                }
                                                for (i, c) in editor.targets.iter().enumerate() {
                                                    let text =
                                                        format!("{} {}", short(&c.hash), c.summary);
                                                    let mark = if hunk.touched.contains(&i) {
                                                        " ●"
                                                    } else {
                                                        ""
                                                    };
                                                    if ui
                                                        .selectable_label(
                                                            choice == Some(i),
                                                            format!("{text}{mark}"),
                                                        )
                                                        .clicked()
                                                    {
                                                        chosen = Some((h, Some(i)));
                                                    }
                                                }
                                            });
                                        ui.label(RichText::new("into").color(t.text_dim));
                                    },
                                );
                            });
                            let (why, color) = verdict(t, editor, hunk.found, &hunk.touched);
                            ui.label(RichText::new(why).color(color).small());
                            for &i in hunk.lines.iter().take(SHOWN) {
                                if let Some(line) = file.lines.get(i) {
                                    code_line(ui, t, line.kind == Kind::Add, &line.text);
                                }
                            }
                            if hunk.lines.len() > SHOWN {
                                ui.label(
                                    RichText::new(format!(
                                        "… {} more lines",
                                        hunk.lines.len() - SHOWN
                                    ))
                                    .color(t.text_faint)
                                    .small(),
                                );
                            }
                        });
                });
                ui.add_space(6.0);
            }
            let whole: Vec<&str> = editor
                .files
                .iter()
                .enumerate()
                .filter(|(f, _)| !editor.hunks.iter().any(|h| h.file == *f))
                .map(|(_, file)| file.path.as_str())
                .collect();
            if !whole.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "Staying staged (new files, or changes that only go whole): {}",
                        whole.join(", ")
                    ))
                    .color(t.text_faint)
                    .small(),
                );
            }
        });
    if let Some((h, target)) = chosen {
        editor.choose(h, target);
    }
}

fn code_line(ui: &mut Ui, t: &Tokens, add: bool, text: &str) {
    let fill = if add { t.diff_add_bg } else { t.diff_del_bg };
    egui::Frame::new().fill(fill).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.add(
            egui::Label::new(
                RichText::new(format!("{}{text}", if add { "+" } else { "-" }))
                    .font(theme::mono(12.0))
                    .color(t.text),
            )
            .truncate(),
        );
    });
}

/// Each commit's fixup and what stays staged, as checked.
fn preview(ui: &mut Ui, t: &Tokens, editor: &AbsorbEditor) {
    let Some(checked) = &editor.checked else {
        return;
    };
    let lines = &checked.preview.lines;
    let title = |g: usize| match checked.fixups.get(g) {
        Some((target, _)) => editor
            .targets
            .get(*target)
            .map(|c| format!("Into {} {}", short(&c.hash), c.summary))
            .unwrap_or_default(),
        None => "Stays staged".to_owned(),
    };
    egui::ScrollArea::both()
        .id_salt(("absorb_preview", editor.id))
        .auto_shrink(false)
        .show_rows(ui, ROW_H, lines.len(), |ui, range| {
            for (look, text) in lines.get(range).unwrap_or_default() {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(400.0), ROW_H),
                    Sense::hover(),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                let ui = &mut child;
                let mono = |c: Color32| RichText::new(text).font(theme::mono(12.0)).color(c);
                let label = match look {
                    Look::Commit(g, _) => {
                        ui.painter().rect_filled(rect, 0.0, t.chrome);
                        ui.add_space(8.0);
                        ui.label(RichText::new(title(*g)).font(theme::medium(13.0)));
                        RichText::new(format!("· {text}")).color(t.text_dim)
                    }
                    Look::Header => mono(t.accent),
                    Look::Meta => mono(t.text_faint),
                    Look::Context => mono(t.text_dim),
                    Look::Add | Look::Del => {
                        let fill = if *look == Look::Add {
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
        });
}
