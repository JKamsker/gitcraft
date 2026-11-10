//! Displayed change rows and file/hunk/line assignment.

use super::*;

/// The old commit's files and lines, each change with the chip of the commit it goes to.
pub(super) fn changes(ui: &mut Ui, t: &Tokens, editor: &mut SplitEditor) {
    let mut click = None;
    let active = editor.active;
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        let lead = if matches!(editor.mode, Mode::Move(_)) {
            crate::i18n::text("repo-workflows-surgery-changes-a2e10493")
        } else {
            crate::i18n::text("repo-workflows-surgery-changes-d37a4bd0")
        };
        ui.label(RichText::new(lead).color(t.text_dim));
        chip(ui, t, active);
        let subject = editor
            .groups
            .get(active)
            .map(|g| g.subject.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::i18n::text("repo-workflows-surgery-changes-4a2b6fd2"));
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

pub(super) fn draw_row(ui: &mut Ui, t: &Tokens, editor: &SplitEditor, row: Row) -> Option<Click> {
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
                crate::i18n::message(
                    "repo-workflows-surgery-draw-row-b23760b8",
                    &[
                        ("file-path-1", (file.path).clone().into()),
                        (
                            "tip-editor-f-editor-units-get-f--2",
                            (tip(
                                editor,
                                f,
                                editor.units.get(f).map_or(&[], Vec::as_slice),
                                crate::i18n::text("repo-workflows-surgery-draw-row-82048030"),
                            ))
                            .clone()
                            .into(),
                        ),
                    ],
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
                    crate::i18n::text("repo-workflows-surgery-draw-row-1b11239e")
                } else {
                    crate::i18n::text("repo-workflows-surgery-draw-row-5359fb45")
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
                    RichText::new(crate::i18n::message(
                        "repo-workflows-surgery-draw-row-f1e7c811",
                        &[("why", (why).into())],
                    ))
                    .color(t.text_faint)
                    .small(),
                );
            }
            if fold.clicked() {
                return Some(Click::Fold(f));
            }
            let units = editor.units.get(f).cloned().unwrap_or_default();
            let resp = resp.on_hover_text(tip(
                editor,
                f,
                &units,
                crate::i18n::text("repo-workflows-surgery-draw-row-82048030"),
            ));
            resp.clicked().then_some(Click::File(f))
        }
        Row::Line(f, i) => {
            let file = editor.files.get(f)?;
            let line = file.lines.get(i)?;
            crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
                crate::i18n::message(
                    "repo-workflows-surgery-draw-row-5d5aa815",
                    &[
                        ("file-path-1", (file.path).clone().into()),
                        ("i-1-2", (i + 1).into()),
                        ("line-text-3", (line.text).clone().into()),
                    ],
                )
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
                Some((
                    crate::i18n::text("repo-workflows-surgery-draw-row-82048030"),
                    Click::File(f),
                ))
            } else if line.kind == Kind::Hunk {
                Some((
                    crate::i18n::text("repo-workflows-surgery-draw-row-bff8613c"),
                    Click::Hunk(f, i),
                ))
            } else if changed {
                Some((
                    crate::i18n::text("repo-workflows-surgery-draw-row-cdd88a94"),
                    Click::Line(f, i),
                ))
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
pub(super) fn tip(editor: &SplitEditor, f: usize, units: &[usize], name: &str) -> String {
    let Mode::Move(m) = &editor.mode else {
        return crate::i18n::message(
            "repo-workflows-surgery-tip-9e1cf75b",
            &[
                ("name", (name).into()),
                ("editor-active-1-1", (editor.active + 1).into()),
            ],
        );
    };
    if !units.is_empty() && units.iter().all(|&u| editor.group_of(f, u) == 1) {
        return crate::i18n::message(
            "repo-workflows-surgery-tip-182c90c3",
            &[
                ("name", (name).into()),
                ("short-editor-hash-1", (short(&editor.hash)).into()),
            ],
        );
    }
    match (&m.dest, m.target()) {
        (_, Some(t)) => crate::i18n::message(
            "repo-workflows-surgery-tip-abb8d5a0",
            &[
                ("name", (name).into()),
                ("short-t-hash-1", (short(&t.hash)).into()),
            ],
        ),
        (Some(Dest::Work), None) => crate::i18n::message(
            "repo-workflows-surgery-tip-9b5f8a9f",
            &[("name", (name).into())],
        ),
        _ => crate::i18n::message(
            "repo-workflows-surgery-tip-4e078e93",
            &[("name", (name).into())],
        ),
    }
}
