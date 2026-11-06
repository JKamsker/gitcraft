//! The split editor in the big area: the new commits and their messages on the left, the old
//! commit's files on the right with a chip per change saying which commit it goes to (click a
//! file, a hunk header or a line to give it to the selected commit), then each new commit's
//! patch to check before splitting.

use egui::{Color32, RichText, Sense, Stroke, Ui};

use super::{MAX_GROUPS, SplitEditor};
use crate::icons;
use crate::repo::diff::Kind;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 20.0;

/// A row of the list of the old commit's changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    File(usize),
    /// Line `1` of file `0`.
    Line(usize, usize),
}

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

impl SplitEditor {
    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (f, file) in self.files.iter().enumerate() {
            rows.push(Row::File(f));
            if !self.folded.contains(&f) {
                rows.extend((0..file.lines.len()).map(|i| Row::Line(f, i)));
            }
        }
        rows
    }
}

/// A chip with the number of new commit `g`, filled with its colour.
fn chip(ui: &mut Ui, t: &Tokens, g: usize) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 15.0), Sense::hover());
    ui.painter().rect_filled(rect, t.radius_sm, color(t, g));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        (g + 1).to_string(),
        theme::semibold(10.5),
        Color32::WHITE,
    );
}

impl RepoTab {
    pub(in crate::repo) fn split_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        let Some(editor) = self.split.as_deref_mut() else {
            return;
        };
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
            .show(ui, |ui| groups(ui, &t, editor));
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

fn header(ui: &mut Ui, t: &Tokens, editor: &SplitEditor, acts: &mut Vec<Action>) {
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0, t.separator))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::inline(ui, "git-branch-plus", 16.0, t.icon);
                ui.label(
                    RichText::new(format!(
                        "Split {} · {}",
                        short(&editor.hash),
                        editor.summary
                    ))
                    .font(theme::medium(13.0)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::small_button(ui, "x", "Cancel (Esc)").clicked() {
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
            ui.horizontal(|ui| {
                let note = match (&editor.error, &problem, &editor.checked) {
                    (Some(e), _, _) => RichText::new(format!("Can't split it so: {e}")).color(t.danger),
                    (None, Some(p), _) => RichText::new(p).color(t.danger),
                    (None, None, Some(c)) => RichText::new(format!(
                        "Checked: {} commits, together exactly the old one. Splitting replays the commits after it · Ctrl+Z undoes it",
                        c.commits.len()
                    ))
                    .color(t.text_dim),
                    (None, None, None) => {
                        RichText::new("Check the split to see each new commit's patch").color(t.text_dim)
                    }
                };
                ui.add(egui::Label::new(note).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if editor.busy {
                        ui.add(crate::spinner::Spinner::new());
                    }
                    if editor.checked.is_some() {
                        if widgets::primary_button(ui, "Split commit", idle && !editor.busy, 120.0)
                            .clicked()
                        {
                            acts.push(Action::RunSplit);
                        }
                        if ui.add_enabled(!editor.busy, egui::Button::new("Back")).clicked() {
                            editor.checked = None;
                        }
                    } else if widgets::primary_button(
                        ui,
                        "Check split",
                        problem.is_none() && !editor.busy,
                        120.0,
                    )
                    .clicked()
                    {
                        acts.push(Action::CheckSplit);
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Action::CloseSplit);
                    }
                });
            });
        });
}

/// The new commits: each with its colour, number of changes and message; the selected one
/// gets what is clicked.
fn groups(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let counts = editor.counts();
    let mut remove = None;
    let before = editor.groups.clone();
    egui::ScrollArea::vertical()
        .id_salt("split_groups_list")
        .auto_shrink(false)
        .show(ui, |ui| {
            if editor.busy {
                ui.disable();
            }
            ui.label(
                RichText::new("Select a commit, then click files, hunks or lines on the right to put them in it.")
                    .color(t.text_dim)
                    .small(),
            );
            ui.add_space(6.0);
            let n = editor.groups.len();
            for g in 0..n {
                let selected = editor.active == g;
                let frame = egui::Frame::new()
                    .fill(if selected { t.row_selected } else { t.field })
                    .stroke(Stroke::new(1.0, if selected { color(t, g) } else { t.field_border }))
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::same(8));
                let shown = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        chip(ui, t, g);
                        let count = counts.get(g).copied().unwrap_or(0);
                        let title = ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Commit {} · {count} change{}",
                                    g + 1,
                                    if count == 1 { "" } else { "s" }
                                ))
                                .font(theme::medium(12.5)),
                            )
                            .sense(Sense::click()),
                        );
                        if title.clicked() {
                            editor.active = g;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if n > 2
                                && icons::small_button(ui, "x", "Take this commit out: its changes go to the one before")
                                    .clicked()
                            {
                                remove = Some(g);
                            }
                        });
                    });
                    let Some(group) = editor.groups.get_mut(g) else {
                        return false;
                    };
                    let subject = ui.add(
                        egui::TextEdit::singleline(&mut group.subject)
                            .hint_text("Summary (required)")
                            .desired_width(f32::INFINITY),
                    );
                    let body = ui.add(
                        egui::TextEdit::multiline(&mut group.body)
                            .hint_text("Description")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    );
                    subject.gained_focus() || body.gained_focus()
                });
                let picked = shown.inner
                    || shown
                        .response
                        .interact(Sense::click())
                        .clicked();
                if picked {
                    editor.active = g;
                }
                ui.add_space(6.0);
            }
            if ui
                .add_enabled(n < MAX_GROUPS, egui::Button::new("Add a commit"))
                .on_disabled_hover_text(format!("At most {MAX_GROUPS} commits"))
                .clicked()
            {
                editor.add_group();
            }
        });
    if let Some(g) = remove {
        editor.remove_group(g);
    } else if editor.groups != before {
        // The checked commits carry the messages they were made with.
        editor.checked = None;
    }
}

/// The old commit's files and lines, each change with the chip of the commit it goes to.
fn changes(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let rows = editor.rows();
    let mut click = None;
    let active = editor.active;
    egui::ScrollArea::both()
        .id_salt("split_changes")
        .auto_shrink(false)
        .show_rows(ui, ROW_H, rows.len(), |ui, range| {
            if editor.busy {
                ui.disable();
            }
            for row in &rows[range] {
                if let Some(c) = draw_row(ui, t, editor, *row) {
                    click = Some(c);
                }
            }
        });
    match click {
        Some(Click::Fold(f)) => {
            if !editor.folded.remove(&f) {
                editor.folded.insert(f);
            }
        }
        Some(Click::File(f)) => editor.assign_file(f, active),
        Some(Click::Hunk(f, h)) => editor.assign_hunk(f, h, active),
        Some(Click::Line(f, i)) => editor.assign_line(f, i, active),
        None => {}
    }
}

fn draw_row(ui: &mut Ui, t: &Tokens, editor: &SplitEditor, row: Row) -> Option<Click> {
    let width = ui.available_width().max(400.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW_H), Sense::click());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    match row {
        Row::File(f) => {
            let file = editor.files.get(f)?;
            ui.painter().rect_filled(rect, 0.0, t.chrome);
            let folded = editor.folded.contains(&f);
            let fold = icons::small_button(
                ui,
                if folded {
                    "chevron-right"
                } else {
                    "chevron-down"
                },
                if folded {
                    "Show its lines"
                } else {
                    "Fold its lines"
                },
            );
            let mut owners: Vec<usize> = file
                .units()
                .iter()
                .map(|&u| editor.group_of(f, u))
                .collect();
            owners.sort_unstable();
            owners.dedup();
            for g in owners {
                chip(ui, t, g);
            }
            ui.label(
                RichText::new(&file.path)
                    .font(theme::medium(12.5))
                    .color(t.text),
            );
            if let Some(why) = file.whole {
                ui.label(
                    RichText::new(format!("whole file only: {why}"))
                        .color(t.text_faint)
                        .small(),
                );
            }
            if fold.clicked() {
                return Some(Click::Fold(f));
            }
            let resp = resp.on_hover_text("Click to put the whole file in the selected commit");
            resp.clicked().then_some(Click::File(f))
        }
        Row::Line(f, i) => {
            let file = editor.files.get(f)?;
            let line = file.lines.get(i)?;
            let changed = matches!(line.kind, Kind::Add | Kind::Del);
            let fill = match line.kind {
                Kind::Add => t.diff_add_bg,
                Kind::Del => t.diff_del_bg,
                _ => Color32::TRANSPARENT,
            };
            ui.painter().rect_filled(rect, 0.0, fill);
            ui.add_space(26.0);
            if changed || (file.whole.is_some() && line.kind != Kind::Hunk) {
                chip(
                    ui,
                    t,
                    editor.group_of(f, if file.whole.is_some() { 0 } else { i }),
                );
            } else {
                ui.add_space(18.0);
            }
            let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
            let numbers = format!("{:>5} {:>5} ", num(line.old), num(line.new));
            let (mark, color) = match line.kind {
                Kind::Add => ("+", t.text),
                Kind::Del => ("-", t.text),
                Kind::Hunk => ("", t.text_faint),
                Kind::Meta => ("", t.text_faint),
                Kind::Context => (" ", t.text_dim),
            };
            ui.label(
                RichText::new(numbers)
                    .font(theme::mono(11.5))
                    .color(t.text_faint),
            );
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{mark}{}", line.text))
                        .font(theme::mono(12.0))
                        .color(color),
                )
                .selectable(false)
                .extend(),
            );
            if !resp.clicked() {
                return None;
            }
            if file.whole.is_some() {
                Some(Click::File(f))
            } else if line.kind == Kind::Hunk {
                Some(Click::Hunk(f, i))
            } else if changed {
                Some(Click::Line(f, i))
            } else {
                None
            }
        }
    }
}

/// Each new commit's patch, as checked.
fn patches(ui: &mut Ui, t: &Tokens, editor: &SplitEditor) {
    let Some(checked) = &editor.checked else {
        return;
    };
    egui::ScrollArea::both()
        .id_salt("split_patches")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(8.0);
            for (g, (patch, files)) in checked.patches.iter().enumerate() {
                let subject = editor
                    .groups
                    .get(g)
                    .map(|g| g.subject.as_str())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    chip(ui, t, g);
                    ui.label(RichText::new(subject).font(theme::medium(13.0)));
                    ui.label(
                        RichText::new(format!(
                            "· {} file{}",
                            files.len(),
                            if files.len() == 1 { "" } else { "s" }
                        ))
                        .color(t.text_dim),
                    );
                });
                egui::CollapsingHeader::new(
                    RichText::new(files.join(", ")).color(t.text_dim).small(),
                )
                .id_salt(("split_patch", g))
                .default_open(checked.patches.len() <= 3)
                .show(ui, |ui| patch_text(ui, t, patch));
                ui.add_space(10.0);
            }
        });
}

/// A patch's lines, coloured. Very long ones are cut: they're for reading over, not applying.
fn patch_text(ui: &mut Ui, t: &Tokens, patch: &str) {
    const MAX: usize = 4000;
    let mut lines = patch.lines();
    for line in lines.by_ref().take(MAX) {
        let (fill, color) = if line.starts_with("+++") || line.starts_with("---") {
            (Color32::TRANSPARENT, t.text_faint)
        } else if line.starts_with('+') {
            (t.diff_add_bg, t.text)
        } else if line.starts_with('-') {
            (t.diff_del_bg, t.text)
        } else if line.starts_with("@@") || line.starts_with("diff ") {
            (Color32::TRANSPARENT, t.accent)
        } else {
            (Color32::TRANSPARENT, t.text_dim)
        };
        egui::Frame::new().fill(fill).show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(line).font(theme::mono(12.0)).color(color)).extend(),
            );
        });
    }
    let rest = lines.count();
    if rest > 0 {
        ui.label(RichText::new(format!("… {rest} more lines")).color(t.text_faint));
    }
}
