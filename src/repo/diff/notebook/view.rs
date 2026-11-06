//! How a notebook's diff looks: a bar to switch between its cells and its JSON and to strip
//! outputs when committing, then each cell that changed with its source diffed and its outputs
//! before and after; unchanged cells folded into a line.

use egui::{RichText, Ui};

use super::NotebookData;
use super::compare::{CellChange, Status};
use super::parse::{Cell, Output};
use crate::repo::Action;
use crate::repo::diff::line_diff::Edit;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

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
            ui.horizontal(|ui| {
                if let Some(i) = widgets::switch(ui, &["Cells", "JSON"], usize::from(json)) {
                    json = i == 1;
                    ui.data_mut(|d| d.insert_temp(key, json));
                }
                ui.label(RichText::new(summary(nb)).color(t.text_dim).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut on = nb.strips;
                    let r = ui
                        .checkbox(&mut on, "Strip outputs when committing")
                        .on_hover_text(
                            "For this repository: what is staged of a notebook has no outputs \
                             or execution counts (a clean filter, like nbstripout); the file on \
                             disk keeps them.",
                        );
                    if r.changed() {
                        acts.push(Action::StripNotebookOutputs(on));
                    }
                });
            });
        });
    if !json {
        cells(ui, &t, nb);
    }
    json
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
    if parts.is_empty() {
        "No cell changed".to_owned()
    } else {
        parts.join(", ")
    }
}

fn cells(ui: &mut Ui, t: &Tokens, nb: &NotebookData) {
    egui::ScrollArea::vertical()
        .id_salt("notebook-cells")
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.add_space(6.0);
            let mut unchanged = 0;
            for (i, c) in nb.diff.cells.iter().enumerate() {
                if c.status == Status::Unchanged {
                    unchanged += 1;
                    continue;
                }
                folded(ui, t, &mut unchanged);
                cell(ui, t, i, c);
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

/// "4 unchanged cells", for those skipped since the last changed one.
fn folded(ui: &mut Ui, t: &Tokens, unchanged: &mut usize) {
    if *unchanged > 0 {
        let n = std::mem::take(unchanged);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            let text = if n == 1 {
                "1 unchanged cell".to_owned()
            } else {
                format!("{n} unchanged cells")
            };
            ui.label(RichText::new(text).color(t.text_faint).small());
        });
        ui.add_space(4.0);
    }
}

fn cell(ui: &mut Ui, t: &Tokens, i: usize, c: &CellChange) {
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
            ui.horizontal(|ui| {
                widgets::chip(ui, word, color);
                let kind = either.map_or("cell", |c| c.kind.label());
                ui.label(RichText::new(format!("{kind} cell {}", i + 1)).font(theme::medium(12.5)));
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
                    .id_salt(("cell-metadata", i))
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
            outputs(ui, t, i, c);
        });
}

/// Lines of a diff, removed red and added green, the rest as they are.
fn lines(ui: &mut Ui, t: &Tokens, lines: &[(Edit, String)]) {
    for (edit, line) in lines {
        let (mark, bg) = match edit {
            Edit::Same => (' ', egui::Color32::TRANSPARENT),
            Edit::Del => ('−', t.diff_del_bg),
            Edit::Add => ('+', t.diff_add_bg),
        };
        egui::Frame::new().fill(bg).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(
                egui::Label::new(RichText::new(format!("{mark} {line}")).font(theme::mono(12.5)))
                    .truncate(),
            );
        });
    }
}

/// The outputs of a code cell: before and after side by side when they changed, else as they
/// are.
fn outputs(ui: &mut Ui, t: &Tokens, i: usize, c: &CellChange) {
    fn of(c: &Option<Cell>) -> &[Output] {
        c.as_ref().map(|c| c.outputs.as_slice()).unwrap_or_default()
    }
    let (old, new) = (of(&c.old), of(&c.new));
    if old.is_empty() && new.is_empty() {
        return;
    }
    ui.add_space(6.0);
    if !c.outputs_changed || old.is_empty() || new.is_empty() {
        let (shown, title) = if new.is_empty() {
            (old, "Outputs (removed)")
        } else {
            (new, "Outputs")
        };
        ui.label(RichText::new(title).color(t.text_dim).small());
        for (k, o) in shown.iter().enumerate() {
            output(ui, t, ("nb-out", i, k), o);
        }
        return;
    }
    ui.columns(2, |cols| {
        for (col, (title, list, side)) in cols
            .iter_mut()
            .zip([("Outputs before", old, 0), ("Outputs after", new, 1)])
        {
            col.label(RichText::new(title).color(t.text_dim).small());
            for (k, o) in list.iter().enumerate() {
                output(col, t, ("nb-out", i * 2 + side, k), o);
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
        Output::Image { mime, bytes } => {
            // By its contents: egui keeps an image by its URI for good.
            let uri = format!(
                "bytes://notebook/{}.{}",
                content_id(bytes),
                mime.rsplit('/').next().unwrap_or("png")
            );
            let max = egui::vec2(ui.available_width().min(480.0), 360.0);
            ui.add(
                egui::Image::from_bytes(uri, egui::load::Bytes::Shared(bytes.clone()))
                    .max_size(max)
                    .fit_to_original_size(1.0)
                    .bg_fill(t.field),
            );
        }
        Output::Other(kinds) => {
            let text = if kinds.is_empty() {
                "(an output gitcraft can't show)".to_owned()
            } else {
                format!("({} output)", kinds.join(", "))
            };
            ui.label(RichText::new(text).color(t.text_faint).small());
        }
    }
}

fn content_id(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}
