//! How a notebook's diff looks: a bar to switch between its cells and its JSON and to strip
//! outputs when committing, then each cell that changed with its source diffed and its outputs
//! before and after; unchanged cells folded into a line that opens.

use egui::{RichText, Ui};

use super::NotebookData;
use super::compare::{CellChange, Status};
use super::parse::{Cell, Output};
use crate::git::StripState;
use crate::repo::Action;
use crate::repo::diff::line_diff::Edit;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

/// Below this width, outputs before and after are stacked rather than side by side.
const SIDE_BY_SIDE: f32 = 640.0;

/// The bar over a notebook's diff; draws the cells (and gives `false`) unless the JSON is
/// picked (`true`: the lines show as usual under it). `id`: the file, which the choice is kept
/// for.
pub(in crate::repo::diff) fn notebook_bar(
    ui: &mut Ui,
    nb: &NotebookData,
    id: &str,
    acts: &mut Vec<Action>,
) -> bool {
    let t = Tokens::get(ui.ctx());
    let key = egui::Id::new(("notebook-json", id));
    let mut json: bool = ui.data(|d| d.get_temp(key)).unwrap_or(false);
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if let Some(i) = widgets::switch(ui, &["Cells", "JSON"], usize::from(json)) {
                    json = i == 1;
                    ui.data_mut(|d| d.insert_temp(key, json));
                }
                ui.label(RichText::new(summary(nb)).color(t.text_dim).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    strip_box(ui, &t, &nb.strip, acts);
                });
            });
            if nb.compared_stripped {
                ui.label(
                    RichText::new("Outputs and execution counts aren't compared: they are stripped when staged.")
                        .color(t.text_faint)
                        .small(),
                );
            }
        });
    if !json {
        cells(ui, &t, nb);
    }
    json
}

/// The "Strip outputs when committing" box, or what strips them when it's another filter.
fn strip_box(ui: &mut Ui, t: &Tokens, strip: &StripState, acts: &mut Vec<Action>) {
    if let StripState::Other(name) = strip {
        ui.label(
            RichText::new(format!(
                "Outputs stripped when committing (by the {name} filter)"
            ))
            .color(t.text_dim)
            .small(),
        )
        .on_hover_text(
            "Set up outside gitcraft (nbstripout --install, or the repository's .gitattributes).",
        );
        return;
    }
    let mut on = matches!(strip, StripState::Ours(_));
    let tool = match strip {
        StripState::Ours(c) if c == "nbstripout" => " It runs nbstripout.",
        StripState::Ours(_) => " It runs Python.",
        _ => "",
    };
    let r = ui
        .checkbox(&mut on, "Strip outputs when committing")
        .on_hover_text(format!(
            "For this clone only: what is staged of a notebook has no outputs, execution counts or \
         widget state (a clean filter, like nbstripout), while the file on disk keeps them. \
         Notebooks that have outputs show as changed until they are committed once so.{tool}"
        ));
    if r.changed() {
        acts.push(Action::StripNotebookOutputs(on));
    }
}

/// "3 cells changed, 1 added, 2 removed".
pub(super) fn summary(nb: &NotebookData) -> String {
    let count = |s: Status| nb.diff.cells.iter().filter(|c| c.status == s).count();
    let parts: Vec<String> = [
        (count(Status::Modified), "changed"),
        (count(Status::Added), "added"),
        (count(Status::Removed), "removed"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .enumerate()
    .map(|(i, (n, what))| {
        if i == 0 {
            format!("{n} {} {what}", if n == 1 { "cell" } else { "cells" })
        } else {
            format!("{n} {what}")
        }
    })
    .collect();
    match (parts.is_empty(), nb.diff.metadata.is_some()) {
        (true, true) => "Only the notebook's metadata changed".to_owned(),
        (true, false) => "No cell changed".to_owned(),
        (false, _) => parts.join(", "),
    }
}

fn cells(ui: &mut Ui, t: &Tokens, nb: &NotebookData) {
    egui::ScrollArea::vertical()
        .id_salt("notebook-cells")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(6.0);
            let mut unchanged: Vec<&CellChange> = Vec::new();
            for c in &nb.diff.cells {
                if c.status == Status::Unchanged {
                    unchanged.push(c);
                    continue;
                }
                folded(ui, t, &mut unchanged);
                cell(ui, t, c);
            }
            folded(ui, t, &mut unchanged);
            if let Some(meta) = &nb.diff.metadata {
                egui::CollapsingHeader::new(
                    RichText::new("The notebook's metadata changed").color(t.text_dim),
                )
                .id_salt("notebook-metadata")
                .show(ui, |ui| lines(ui, t, meta));
            }
        });
}

/// "4 unchanged cells", for those skipped since the last changed one; open, their sources.
fn folded(ui: &mut Ui, t: &Tokens, unchanged: &mut Vec<&CellChange>) {
    if unchanged.is_empty() {
        return;
    }
    let cells = std::mem::take(unchanged);
    let first = cells[0].number;
    let text = if cells.len() == 1 {
        "1 unchanged cell".to_owned()
    } else {
        format!("{} unchanged cells", cells.len())
    };
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        egui::CollapsingHeader::new(RichText::new(text).color(t.text_faint).small())
            .id_salt((crate::i18n::text("repo-core-0610"), first))
            .show(ui, |ui| {
                for c in cells {
                    let Some(cell) = c.new.as_ref() else { continue };
                    ui.label(
                        RichText::new(format!("{} cell {}", cell.kind.label(), c.number))
                            .color(t.text_dim)
                            .small(),
                    );
                    for line in cell.source.lines().take(40) {
                        ui.add(
                            egui::Label::new(
                                RichText::new(line)
                                    .font(theme::mono(12.0))
                                    .color(t.text_dim),
                            )
                            .truncate(),
                        );
                    }
                }
            });
    });
    ui.add_space(4.0);
}

fn cell(ui: &mut Ui, t: &Tokens, c: &CellChange) {
    let (word, color) = match c.status {
        Status::Added => ("added", t.success),
        Status::Removed => ("removed", t.danger),
        Status::Modified => ("changed", t.warning),
        Status::Unchanged => ("", t.text_dim),
    };
    let either = c.new.as_ref().or(c.old.as_ref());
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, t.separator))
        .corner_radius(4)
        .inner_margin(egui::Margin::same(8))
        .outer_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, word, color);
                if c.moved {
                    widgets::chip(ui, "moved", t.text_dim);
                }
                let kind = either.map_or("cell", |c| c.kind.label());
                ui.label(
                    RichText::new(format!("{kind} cell {}", c.number)).font(theme::medium(12.5)),
                );
                let count = |c: &Option<Cell>| {
                    c.as_ref()
                        .and_then(|c| c.execution_count)
                        .map_or(" ".to_owned(), |n| n.to_string())
                };
                if either.is_some_and(|c| c.execution_count.is_some()) || c.count_changed {
                    let text = if c.count_changed {
                        format!("In [{}] → [{}]", count(&c.old), count(&c.new))
                    } else {
                        format!("In [{}]", count(&c.new))
                    };
                    ui.label(RichText::new(text).color(t.text_faint).small());
                }
                if c.metadata_changed {
                    ui.label(
                        RichText::new("metadata changed")
                            .color(t.text_faint)
                            .small(),
                    );
                }
            });
            ui.add_space(4.0);
            lines(ui, t, &c.source);
            if c.metadata_changed {
                egui::CollapsingHeader::new(RichText::new("Metadata").color(t.text_dim).small())
                    .id_salt(("cell-metadata", c.number, word))
                    .show(ui, |ui| {
                        let (a, b) = (
                            c.old.as_ref().map_or("", |c| c.metadata.as_str()),
                            c.new.as_ref().map_or("", |c| c.metadata.as_str()),
                        );
                        let (a, b): (Vec<&str>, Vec<&str>) =
                            (a.lines().collect(), b.lines().collect());
                        let d: Vec<(Edit, String)> =
                            crate::repo::diff::line_diff::diff_lines(&a, &b)
                                .into_iter()
                                .map(|(e, l)| (e, l.to_owned()))
                                .collect();
                        lines(ui, t, &d);
                    });
            }
            outputs(ui, t, c);
        });
}

/// Lines of a diff, removed red and added green, the rest as they are; a long line wraps.
fn lines(ui: &mut Ui, t: &Tokens, lines: &[(Edit, String)]) {
    for (edit, line) in lines {
        let (mark, bg, color) = match edit {
            Edit::Same => (" ", egui::Color32::TRANSPARENT, t.text_faint),
            Edit::Del => ("−", t.diff_del_bg, t.danger),
            Edit::Add => ("+", t.diff_add_bg, t.success),
        };
        egui::Frame::new().fill(bg).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(mark).font(theme::mono(12.5)).color(color));
                ui.add(egui::Label::new(RichText::new(line).font(theme::mono(12.5))).wrap());
            });
        });
    }
}

/// The outputs of a code cell: before and after (side by side when there is room) when they
/// changed, else as they are.
fn outputs(ui: &mut Ui, t: &Tokens, c: &CellChange) {
    fn of(c: &Option<Cell>) -> &[Output] {
        c.as_ref().map(|c| c.outputs.as_slice()).unwrap_or_default()
    }
    let (old, new) = (of(&c.old), of(&c.new));
    if old.is_empty() && new.is_empty() {
        return;
    }
    ui.add_space(6.0);
    let n = c.number;
    if !c.outputs_changed || old.is_empty() || new.is_empty() {
        let (shown, title) = if new.is_empty() {
            (old, "Outputs (removed)")
        } else if old.is_empty() && c.old.is_some() {
            (new, "Outputs (added)")
        } else {
            (new, "Outputs")
        };
        ui.label(RichText::new(title).color(t.text_dim).small());
        for (k, o) in shown.iter().enumerate() {
            output(ui, t, ("nb-out", n, k), o);
        }
        return;
    }
    let sides = [("Outputs before", old, 0), ("Outputs after", new, 1)];
    if ui.available_width() < SIDE_BY_SIDE {
        for (title, list, side) in sides {
            ui.label(RichText::new(title).color(t.text_dim).small());
            for (k, o) in list.iter().enumerate() {
                output(ui, t, ("nb-out", n * 2 + side, k), o);
            }
        }
        return;
    }
    ui.columns(2, |cols| {
        for (col, (title, list, side)) in cols.iter_mut().zip(sides) {
            col.label(RichText::new(title).color(t.text_dim).small());
            for (k, o) in list.iter().enumerate() {
                output(col, t, ("nb-out", n * 2 + side, k), o);
            }
        }
    });
}

fn output(ui: &mut Ui, t: &Tokens, id: (&str, usize, usize), o: &Output) {
    let mono = |text: &str| RichText::new(text.trim_end()).font(theme::mono(12.0));
    match o {
        Output::Stream { name, text } => {
            let color = if name == "stderr" { t.warning } else { t.text };
            ui.add(egui::Label::new(mono(text).color(color)).wrap());
        }
        Output::Text(text) => {
            ui.add(egui::Label::new(mono(text)).wrap());
        }
        Output::Error {
            name,
            value,
            traceback,
        } => {
            ui.add(egui::Label::new(mono(&format!("{name}: {value}")).color(t.danger)).wrap());
            if !traceback.is_empty() {
                egui::CollapsingHeader::new(RichText::new("Traceback").small())
                    .id_salt(id)
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(mono(traceback)).wrap());
                    });
            }
        }
        Output::Image { bytes, uri, .. } => {
            let max = egui::vec2(ui.available_width().min(480.0), 360.0);
            ui.add(
                egui::Image::from_bytes(uri.clone(), egui::load::Bytes::Shared(bytes.clone()))
                    .max_size(max)
                    .fit_to_original_size(1.0)
                    .bg_fill(t.field),
            );
        }
        Output::Other(kinds, _) => {
            let text = if kinds.is_empty() {
                "(an output gitcraft can't show)".to_owned()
            } else {
                format!("({} output)", kinds.join(", "))
            };
            ui.label(RichText::new(text).color(t.text_faint).small());
        }
    }
}
