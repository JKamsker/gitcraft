//! The split editor in the big area: the new commits and their messages on the left, the old
//! commit's files on the right with a chip per change saying which commit it goes to (click a
//! file, a hunk header or a line to give it to the selected commit; 1–9 select a commit), then
//! each new commit's patch to check before splitting.

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
                    Mode::Split => "Split",
                    Mode::Move(_) => "Move changes out of",
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
            // The buttons first, from the right: the note takes what is left, cut short, all
            // of it in its tooltip.
            let moving = matches!(editor.mode, Mode::Move(_));
            let (check, run) = if moving {
                ("Check move", "Move changes")
            } else {
                ("Check split", "Split commit")
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if editor.checked.is_some() {
                    if widgets::primary_button(ui, run, idle && !editor.busy, 120.0).clicked() {
                        acts.push(Action::RunSplit);
                    }
                    if ui
                        .add_enabled(!editor.busy, egui::Button::new("Back"))
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
        (Some(e), _, _) if matches!(editor.mode, Mode::Move(_)) => {
            (format!("Can't move them this way: {e}"), t.danger)
        }
        (Some(e), _, _) => (format!("Can't split it this way: {e}"), t.danger),
        (None, Some((p, hint)), _) => (p, if hint { t.text_dim } else { t.danger }),
        (None, None, Some(_)) if matches!(editor.mode, Mode::Move(_)) => (
            "Checked: what stays and what moves. Moving rewrites the branch from the older commit · Ctrl+Z undoes it".to_owned(),
            t.text_dim,
        ),
        (None, None, Some(c)) => (
            format!(
                "Checked: {} commits, together exactly the old one. Splitting replays the commits after it · Ctrl+Z undoes it",
                c.trees.len().saturating_sub(1)
            ),
            t.text_dim,
        ),
        (None, None, None) if matches!(editor.mode, Mode::Move(_)) => (
            "Check the move to see what stays in the commit and what moves".to_owned(),
            t.text_dim,
        ),
        (None, None, None) => (
            "Check the split to see each new commit's patch".to_owned(),
            t.text_dim,
        ),
    }
}

/// The new commits: each with its colour, number of changes and message; the selected one
/// gets what is clicked.
fn groups(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let mut remove = None;
    egui::ScrollArea::vertical()
        .id_salt("split_groups_list")
        .auto_shrink(false)
        .show(ui, |ui| {
            if editor.busy {
                ui.disable();
            }
            ui.label(
                RichText::new(
                    "Select a commit (or press its number), then click files, hunks or lines on the right to put them in it.",
                )
                .color(t.text_dim)
                .small(),
            );
            ui.add_space(6.0);
            let n = editor.groups.len();
            for g in 0..n {
                ui.push_id(("split_group", editor.id, g), |ui| {
                    if group_card(ui, t, editor, g, &mut remove) {
                        editor.active = g;
                    }
                });
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
    }
}

/// New commit `g`'s card; whether it was clicked (or typed in), which selects it.
fn group_card(
    ui: &mut Ui,
    t: &Tokens,
    editor: &mut SplitEditor,
    g: usize,
    remove: &mut Option<usize>,
) -> bool {
    let n = editor.groups.len();
    let selected = editor.active == g;
    let count = editor.counts.get(g).copied().unwrap_or(0);
    let frame = egui::Frame::new()
        .fill(if selected { t.row_selected } else { t.field })
        .stroke(Stroke::new(
            1.0,
            if selected {
                color(t, g)
            } else {
                t.field_border
            },
        ))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8));
    let shown = frame.show(ui, |ui| {
        let mut picked = false;
        ui.horizontal(|ui| {
            chip(ui, t, g);
            let title = ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "Commit {} · {count} change{}",
                        g + 1,
                        if count == 1 { "" } else { "s" }
                    ))
                    .font(theme::medium(12.5)),
                )
                .selectable(false)
                .sense(Sense::click()),
            );
            picked |= title.clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let heir = SplitEditor::heir(g);
                let tip = if heir == g {
                    "Take this commit out: its changes go to the one after it".to_owned()
                } else {
                    format!(
                        "Take this commit out: its changes go to commit {}",
                        heir + 1
                    )
                };
                if n > 2 && icons::small_button(ui, "x", &tip).clicked() {
                    *remove = Some(g);
                }
            });
        });
        let Some(group) = editor.groups.get_mut(g) else {
            return picked;
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
        picked || subject.gained_focus() || body.gained_focus()
    });
    shown.inner || shown.response.interact(Sense::click()).clicked()
}

/// Where moved changes can go: the other commits of the branch, newest first, or the working
/// changes.
fn destinations(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let Mode::Move(m) = &editor.mode else {
        return;
    };
    let mut picked = None;
    ui.label(
        RichText::new(
            "Click the files, hunks or lines to move on the right (chip 2; click again to keep them), then pick where they go.",
        )
        .color(t.text_dim)
        .small(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        chip(ui, t, 0);
        ui.label(RichText::new(&editor.groups[0].subject).color(t.text_dim));
    });
    ui.horizontal(|ui| {
        chip(ui, t, 1);
        ui.add(egui::Label::new(RichText::new(&editor.groups[1].subject)).truncate());
    });
    ui.add_space(8.0);
    if editor.busy {
        ui.disable();
    }
    if ui
        .selectable_label(m.dest == Some(Dest::Work), "Working changes (staged)")
        .on_hover_text("Out of the commit, staged; their files must have no other changes")
        .clicked()
    {
        picked = Some(Dest::Work);
    }
    ui.separator();
    // Newest first, the commit itself in its place (not a choice): above it newer commits,
    // below it older ones, from which more of the branch is rewritten.
    let rows = m.targets.len() + 1 + usize::from(m.truncated);
    let mine = m.targets.iter().take_while(|t| t.depth < m.depth).count();
    egui::ScrollArea::vertical()
        .id_salt(("move_targets", editor.id))
        .auto_shrink(false)
        .show_rows(ui, 22.0, rows, |ui, range| {
            for row in range {
                if row == mine {
                    ui.label(
                        RichText::new(format!("{}  this commit", short(&editor.hash)))
                            .color(t.text_faint)
                            .italics(),
                    );
                    continue;
                }
                let i = if row < mine { row } else { row - 1 };
                let Some(target) = m.targets.get(i) else {
                    ui.label(
                        RichText::new(
                            "Older commits aren't loaded yet: scroll the graph down to load them.",
                        )
                        .color(t.text_faint)
                        .small(),
                    );
                    continue;
                };
                let text = format!("{}  {}", short(&target.hash), target.summary);
                if ui
                    .add(egui::Button::selectable(m.dest == Some(Dest::Commit(i)), text).truncate())
                    .clicked()
                {
                    picked = Some(Dest::Commit(i));
                }
            }
        });
    if m.targets.is_empty() && !m.truncated {
        ui.label(
            RichText::new(
                "No other commit of the branch can take them: a merge or the first commit is next.",
            )
            .color(t.text_faint)
            .small(),
        );
    }
    if let Some(dest) = picked {
        editor.set_dest(dest);
    }
}

/// The old commit's files and lines, each change with the chip of the commit it goes to.
fn changes(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let mut click = None;
    let active = editor.active;
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let lead = if matches!(editor.mode, Mode::Move(_)) {
            "Clicks move to"
        } else {
            "Clicks go to"
        };
        ui.label(RichText::new(lead).color(t.text_dim));
        chip(ui, t, active);
        let subject = editor
            .groups
            .get(active)
            .map(|g| g.subject.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("(no summary yet)");
        ui.add(egui::Label::new(RichText::new(subject).color(t.text)).truncate());
    });
    let rows = std::mem::take(&mut editor.rows);
    egui::ScrollArea::both()
        .id_salt(("split_changes", editor.id))
        .auto_shrink(false)
        .show_rows(ui, ROW_H, rows.len(), |ui, range| {
            if editor.busy {
                ui.disable();
            }
            for row in rows.get(range).unwrap_or_default() {
                if let Some(c) = draw_row(ui, t, editor, *row) {
                    click = Some(c);
                }
            }
        });
    editor.rows = rows;
    // Moving, a click on what already moves puts it back.
    let to = |editor: &SplitEditor, units: &[usize], f: usize| {
        let moving = matches!(editor.mode, Mode::Move(_));
        if moving && units.iter().all(|&u| editor.group_of(f, u) == 1) {
            0
        } else {
            active
        }
    };
    match click {
        Some(Click::Fold(f)) => editor.toggle_fold(f),
        Some(Click::File(f)) => {
            let units = editor.units.get(f).cloned().unwrap_or_default();
            let g = to(editor, &units, f);
            editor.assign_file(f, g);
        }
        Some(Click::Hunk(f, h)) => {
            let range = editor.hunk_range(f, h);
            let units: Vec<usize> = editor
                .units
                .get(f)
                .into_iter()
                .flatten()
                .copied()
                .filter(|u| range.contains(u))
                .collect();
            let g = to(editor, &units, f);
            editor.assign_hunk(f, h, g);
        }
        Some(Click::Line(f, i)) => {
            let g = to(editor, &[i], f);
            editor.assign_line(f, i, g);
        }
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
            ui.painter()
                .rect_filled(rect, 0.0, if resp.hovered() { t.hover } else { t.chrome });
            let folded = editor.folded.contains(&f);
            crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
                format!(
                    "Assign file {}: {}",
                    file.path,
                    tip(
                        editor,
                        f,
                        editor.units.get(f).map_or(&[], Vec::as_slice),
                        "the whole file"
                    )
                )
            });
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
            let mut owners: Vec<usize> = editor
                .units
                .get(f)
                .into_iter()
                .flatten()
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
            let units = editor.units.get(f).cloned().unwrap_or_default();
            let resp = resp.on_hover_text(tip(editor, f, &units, "the whole file"));
            resp.clicked().then_some(Click::File(f))
        }
        Row::Line(f, i) => {
            let file = editor.files.get(f)?;
            let line = file.lines.get(i)?;
            crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
                format!("Assign {} line {}: {}", file.path, i + 1, line.text)
            });
            let changed = matches!(line.kind, Kind::Add | Kind::Del);
            let whole = file.whole.is_some();
            let clickable = whole || changed || line.kind == Kind::Hunk;
            let fill = match line.kind {
                Kind::Add => t.diff_add_bg,
                Kind::Del => t.diff_del_bg,
                _ => Color32::TRANSPARENT,
            };
            ui.painter().rect_filled(rect, 0.0, fill);
            if clickable && resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 0.0, t.hover.gamma_multiply(0.6));
            }
            ui.add_space(26.0);
            if changed || (whole && line.kind != Kind::Hunk) {
                chip(ui, t, editor.group_of(f, if whole { 0 } else { i }));
            } else {
                ui.add_space(18.0);
            }
            let num = |n: Option<u32>| n.map(|n| n.to_string()).unwrap_or_default();
            let numbers = format!("{:>5} {:>5} ", num(line.old), num(line.new));
            let (mark, color) = match line.kind {
                Kind::Add => ("+", t.text),
                Kind::Del => ("-", t.text),
                Kind::Hunk | Kind::Meta | Kind::Fold => ("", t.text_faint),
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
            let what = if whole {
                Some(("the whole file", Click::File(f)))
            } else if line.kind == Kind::Hunk {
                Some(("this hunk", Click::Hunk(f, i)))
            } else if changed {
                Some(("this line", Click::Line(f, i)))
            } else {
                None
            };
            let (name, click) = what?;
            let units: Vec<usize> = match click {
                Click::Hunk(_, h) => {
                    let range = editor.hunk_range(f, h);
                    editor
                        .units
                        .get(f)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|u| range.contains(u))
                        .collect()
                }
                Click::File(_) => vec![0],
                _ => vec![i],
            };
            let resp = resp.on_hover_text(tip(editor, f, &units, name));
            resp.clicked().then_some(click)
        }
    }
}

/// What a click on `name` (units `units` of file `f`) does.
fn tip(editor: &SplitEditor, f: usize, units: &[usize], name: &str) -> String {
    let Mode::Move(m) = &editor.mode else {
        return format!("Put {name} in commit {}", editor.active + 1);
    };
    if !units.is_empty() && units.iter().all(|&u| editor.group_of(f, u) == 1) {
        return format!("Keep {name} in {}", short(&editor.hash));
    }
    match (&m.dest, m.target()) {
        (_, Some(t)) => format!("Move {name} to {}", short(&t.hash)),
        (Some(Dest::Work), None) => format!("Move {name} to the working changes"),
        _ => format!("Move {name} (pick where on the left)"),
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
                RichText::new(format!("· nothing: {} is dropped", short(&editor.hash)))
                    .color(t.text_dim)
            } else {
                RichText::new(format!(
                    "· {files} file{}: {text}",
                    if files == 1 { "" } else { "s" }
                ))
                .color(t.text_dim)
            }
        }
        Look::Header => mono(t.accent),
        Look::Meta => mono(t.text_faint),
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
