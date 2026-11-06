//! How a binary file's diff looks: its kind and sizes before and after with what is known of
//! each side, then its text, its archive's members or its bytes, as picked.

use egui::{RichText, Ui};

use super::{BinaryPair, HEX_BYTES, MemberChange, Side};
use crate::git::human_size;
use crate::icons;
use crate::repo::diff::line_diff::Edit;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

/// What the lower part shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Show {
    #[default]
    Text,
    Members,
    Hex,
}

/// The diff of a binary file. `id`: the file shown, which the view picked is kept for.
pub(in crate::repo::diff) fn binary_view(ui: &mut Ui, pair: &BinaryPair, id: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            summary(ui, &t, pair);
            if let Some(e) = &pair.error {
                ui.colored_label(t.danger, e);
            }
            ui.add_space(10.0);
            let options: Vec<(Show, &str)> = [
                (Show::Text, "Text", pair.text.is_some()),
                (Show::Members, "Members", pair.members.is_some()),
                (Show::Hex, "Bytes", has_bytes(pair)),
            ]
            .into_iter()
            .filter(|(_, _, there)| *there)
            .map(|(s, l, _)| (s, l))
            .collect();
            if options.is_empty() {
                return;
            }
            let key = egui::Id::new(("binary-view", id));
            let mut show: Show = ui.data(|d| d.get_temp(key)).unwrap_or_default();
            if !options.iter().any(|(s, _)| *s == show) {
                show = options[0].0;
            }
            ui.horizontal(|ui| {
                let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
                let on = options.iter().position(|(s, _)| *s == show).unwrap_or(0);
                if let Some(i) = widgets::switch(ui, &labels, on) {
                    show = options[i].0;
                }
            });
            ui.data_mut(|d| d.insert_temp(key, show));
            ui.add_space(6.0);
            match show {
                Show::Text => text_diff(ui, &t, pair),
                Show::Members => members(ui, &t, pair.members.as_deref().unwrap_or_default()),
                Show::Hex => hex(ui, &t, pair),
            }
        });
}

fn has_bytes(pair: &BinaryPair) -> bool {
    [&pair.old, &pair.new]
        .iter()
        .any(|s| s.as_ref().is_some_and(|s| !s.head.is_empty()))
}

/// "+1.8 KB", "−300 B", "same size".
pub(super) fn size_change(old: u64, new: u64) -> String {
    match new.cmp(&old) {
        std::cmp::Ordering::Equal => "same size".to_owned(),
        std::cmp::Ordering::Greater => format!("+{}", human_size(new - old)),
        std::cmp::Ordering::Less => format!("−{}", human_size(old - new)),
    }
}

/// Kind, sizes and facts of both sides, facts that changed marked.
fn summary(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let kind = pair
        .new
        .as_ref()
        .or(pair.old.as_ref())
        .and_then(|s| s.kind.clone())
        .unwrap_or_else(|| "Binary file".to_owned());
    ui.horizontal(|ui| {
        icons::inline(ui, "file-diff", 16.0, t.icon);
        ui.label(RichText::new(kind).font(theme::semibold(14.0)));
        if let (Some(a), Some(b)) = (&pair.old, &pair.new) {
            let change = size_change(a.size, b.size);
            ui.label(
                RichText::new(format!(
                    "{} → {} ({change})",
                    human_size(a.size),
                    human_size(b.size)
                ))
                .color(t.text_dim),
            );
        }
    });
    ui.add_space(6.0);
    let mut facts: Vec<String> = vec!["Size".to_owned()];
    for s in [&pair.old, &pair.new].into_iter().flatten() {
        for (k, _) in &s.facts {
            if !facts.contains(k) {
                facts.push(k.clone());
            }
        }
    }
    let value = |s: &Option<Side>, k: &str| -> Option<String> {
        let s = s.as_ref()?;
        if k == "Size" {
            return Some(
                human_size(s.size)
                    + &if s.kind.is_none() {
                        " (too big to read)".to_owned()
                    } else {
                        String::new()
                    },
            );
        }
        s.facts.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone())
    };
    egui::Grid::new("binary-facts")
        .num_columns(3)
        .spacing([18.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            ui.label(
                RichText::new("Before")
                    .font(theme::semibold(12.0))
                    .color(t.text_dim),
            );
            ui.label(
                RichText::new("After")
                    .font(theme::semibold(12.0))
                    .color(t.text_dim),
            );
            ui.end_row();
            for k in &facts {
                let (a, b) = (value(&pair.old, k), value(&pair.new, k));
                let changed = a != b;
                ui.label(RichText::new(k).color(t.text_dim));
                let missing = |side_there: bool| if side_there { "—" } else { "(no file)" };
                let shown = |v: &Option<String>, there: bool, color| {
                    RichText::new(v.clone().unwrap_or_else(|| missing(there).to_owned()))
                        .color(color)
                };
                let (ca, cb) = if changed {
                    (t.danger, t.success)
                } else {
                    (t.text, t.text)
                };
                ui.label(shown(&a, pair.old.is_some(), ca));
                ui.label(shown(&b, pair.new.is_some(), cb));
                ui.end_row();
            }
        });
}

fn text_diff(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let Some(lines) = &pair.text else { return };
    let changes = lines.iter().filter(|(e, _)| *e != Edit::Same).count();
    if changes == 0 {
        ui.label(
            RichText::new("The text is the same: what changed is elsewhere in the file.")
                .color(t.text_dim),
        );
    }
    let row_h = 18.0;
    egui::ScrollArea::both()
        .id_salt("binary-text")
        .auto_shrink(false)
        .show_rows(ui, row_h, lines.len(), |ui, range| {
            for (edit, line) in &lines[range] {
                let (mark, bg, color) = match edit {
                    Edit::Same => (' ', egui::Color32::TRANSPARENT, t.text),
                    Edit::Del => ('−', t.diff_del_bg, t.text),
                    Edit::Add => ('+', t.diff_add_bg, t.text),
                };
                egui::Frame::new().fill(bg).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        RichText::new(format!("{mark} {line}"))
                            .font(theme::mono(12.5))
                            .color(color),
                    );
                });
            }
        });
}

fn members(ui: &mut Ui, t: &Tokens, changes: &[MemberChange]) {
    if changes.is_empty() {
        ui.label(RichText::new("Every member is the same.").color(t.text_dim));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("binary-members")
        .auto_shrink(false)
        .show_rows(ui, 20.0, changes.len(), |ui, range| {
            for c in &changes[range] {
                ui.horizontal(|ui| {
                    let (word, color, size) = match (c.old, c.new) {
                        (None, Some(n)) => ("added", t.success, human_size(n)),
                        (Some(o), None) => ("removed", t.danger, human_size(o)),
                        (Some(o), Some(n)) => (
                            "changed",
                            t.warning,
                            format!(
                                "{} → {} ({})",
                                human_size(o),
                                human_size(n),
                                size_change(o, n)
                            ),
                        ),
                        (None, None) => ("", t.text_dim, String::new()),
                    };
                    widgets::chip(ui, word, color);
                    ui.label(RichText::new(&c.name).font(theme::mono(12.5)));
                    ui.label(RichText::new(size).color(t.text_faint).small());
                });
            }
        });
}

/// 16 bytes a row, both sides one under the other per row; bytes that differ marked.
fn hex(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let empty = Vec::new();
    let old = pair.old.as_ref().map_or(&empty, |s| &s.head);
    let new = pair.new.as_ref().map_or(&empty, |s| &s.head);
    let rows = old.len().max(new.len()).div_ceil(16);
    let cut = [pair.old.as_ref(), pair.new.as_ref()]
        .into_iter()
        .flatten()
        .any(|s| s.size > HEX_BYTES as u64);
    if cut {
        ui.label(
            RichText::new(format!(
                "The first {} of each side.",
                human_size(HEX_BYTES as u64)
            ))
            .color(t.text_faint)
            .small(),
        );
    }
    egui::ScrollArea::both()
        .id_salt("binary-hex")
        .auto_shrink(false)
        .show_rows(ui, 34.0, rows, |ui, range| {
            for row in range {
                let at = row * 16;
                ui.vertical(|ui| {
                    for (mark, bytes, other, color) in
                        [('−', old, new, t.danger), ('+', new, old, t.success)]
                    {
                        let mut job = egui::text::LayoutJob::default();
                        let font = theme::mono(12.0);
                        let plain = egui::TextFormat {
                            font_id: font.clone(),
                            color: t.text_dim,
                            ..Default::default()
                        };
                        job.append(&format!("{mark} {at:08x}  "), 0.0, plain.clone());
                        let mut ascii = String::new();
                        for i in at..at + 16 {
                            let (text, differs) = match bytes.get(i) {
                                Some(b) => {
                                    ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                                        char::from(*b)
                                    } else {
                                        '·'
                                    });
                                    (format!("{b:02x} "), other.get(i) != Some(b))
                                }
                                None => ("   ".to_owned(), false),
                            };
                            let format = if differs {
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color,
                                    ..Default::default()
                                }
                            } else {
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color: t.text,
                                    ..Default::default()
                                }
                            };
                            job.append(&text, 0.0, format);
                        }
                        job.append(&format!(" {ascii}"), 0.0, plain);
                        ui.label(job);
                    }
                });
            }
        });
}

/// A line above a diff git made through the file's `textconv` program: the text is that
/// program's, and the patch can't be staged a line at a time.
pub(in crate::repo::diff) fn textconv_note(ui: &mut Ui, program: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!(
                    "The text of this file as `{program}` (its textconv) gives it; it is staged as a whole."
                ))
                .color(t.text_dim)
                .small(),
            );
        });
}
