//! How a binary file's diff looks: its kind and what changed in its size, a table of what is
//! known of each side, then its text, its archive's members or its bytes, as picked.

use egui::{RichText, Ui};

use super::{BinaryPair, HEX_BYTES, MAX_READ, MemberChange, Side};
use crate::git::{Area, human_size};
use crate::icons;
use crate::repo::diff::line_diff::Edit;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

/// What the lower part shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Show {
    Text,
    Members,
    Hex,
}

/// The diff of a binary file. `id`: the file shown, which the view picked is kept for.
pub(in crate::repo::diff) fn binary_view(ui: &mut Ui, pair: &BinaryPair, id: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 0))
        .show(ui, |ui| {
            summary(ui, &t, pair);
            if let Some(e) = &pair.error {
                ui.colored_label(t.danger, e);
            }
            ui.add_space(10.0);
            let mut options: Vec<(Show, &str, bool)> = Vec::new();
            if let Some(text) = &pair.text {
                options.push((
                    Show::Text,
                    crate::i18n::text("repo-core-1683"),
                    !text.is_empty() && pair.text_changes > 0,
                ));
            }
            if let Some(m) = &pair.members {
                options.push((
                    Show::Members,
                    crate::i18n::text("repo-core-1598"),
                    !m.is_empty(),
                ));
            }
            if has_bytes(pair) {
                options.push((Show::Hex, crate::i18n::text("repo-core-1684"), true));
            }
            for note in [&pair.text_note, &pair.members_note].into_iter().flatten() {
                ui.label(RichText::new(note).color(t.text_dim).small());
            }
            if options.is_empty() {
                if [&pair.old, &pair.new]
                    .iter()
                    .any(|s| s.as_ref().is_some_and(|s| s.kind.is_none() && !s.failed))
                {
                    let text = crate::i18n::message(
                        "repo-core-1685",
                        &[("MAX-READ", format!("{}", human_size(MAX_READ)).into())],
                    );
                    ui.label(RichText::new(text).color(t.text_dim));
                }
                return;
            }
            let key = egui::Id::new(("binary-view", id));
            let picked: Option<Show> = ui.data(|d| d.get_temp(key));
            // The pick if this file has it, else the first view with changes, else the first.
            let show = picked
                .filter(|p| options.iter().any(|(s, _, _)| s == p))
                .or_else(|| {
                    options
                        .iter()
                        .find(|(_, _, changed)| *changed)
                        .map(|(s, _, _)| *s)
                })
                .unwrap_or(options[0].0);
            let mut show = show;
            if options.len() > 1 {
                ui.horizontal(|ui| {
                    let labels: Vec<&str> = options.iter().map(|(_, l, _)| *l).collect();
                    let on = options.iter().position(|(s, _, _)| *s == show).unwrap_or(0);
                    if let Some(i) = widgets::switch(ui, &labels, on) {
                        show = options[i].0;
                        ui.data_mut(|d| d.insert_temp(key, show));
                    }
                });
                ui.add_space(6.0);
            }
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
        std::cmp::Ordering::Equal => crate::i18n::text("repo-core-1687").to_owned(),
        std::cmp::Ordering::Greater => format!("+{}", human_size(new - old)),
        std::cmp::Ordering::Less => format!("−{}", human_size(old - new)),
    }
}

/// A side's size as the table says it: "12.0 KB", "70.2 MB, not read (over 64 MB)".
pub(super) fn size_text(s: &Side) -> String {
    if s.failed {
        return crate::i18n::text("repo-core-1688").to_owned();
    }
    let limit = human_size(MAX_READ);
    match (s.size, s.kind.is_some()) {
        (Some(n), true) => human_size(n),
        (Some(n), false) => crate::i18n::message(
            "repo-core-1689",
            &[
                ("n", format!("{}", human_size(n)).into()),
                ("limit", format!("{}", limit).into()),
            ],
        ),
        (None, _) => {
            crate::i18n::message("repo-core-1690", &[("limit", format!("{}", limit).into())])
        }
    }
}

/// Kind and the size's change, then the facts of both sides, those that changed marked.
fn summary(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let kind = |s: &Option<Side>| s.as_ref().and_then(|s| s.kind.clone());
    let kinds = match (kind(&pair.old), kind(&pair.new)) {
        (Some(a), Some(b)) if a != b => format!("{a} → {b}"),
        (a, b) => b
            .or(a)
            .unwrap_or_else(|| crate::i18n::text("repo-core-1600").to_owned()),
    };
    ui.horizontal(|ui| {
        icons::inline(ui, "file-diff", 16.0, t.icon);
        ui.label(RichText::new(kinds).font(theme::semibold(14.0)));
        if let (Some(a), Some(b)) = (
            pair.old.as_ref().and_then(|s| s.size),
            pair.new.as_ref().and_then(|s| s.size),
        ) {
            ui.label(RichText::new(size_change(a, b)).color(t.text_dim));
        }
    });
    ui.add_space(6.0);
    let mut facts: Vec<String> = vec!["repo-core-1693".to_owned()];
    for s in [&pair.old, &pair.new].into_iter().flatten() {
        for (k, _) in &s.facts {
            if !facts.contains(k) {
                facts.push(k.clone());
            }
        }
    }
    let value = |s: &Option<Side>, k: &str| -> Option<String> {
        let s = s.as_ref()?;
        if k == "repo-core-1693" {
            return Some(size_text(s));
        }
        s.facts.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone())
    };
    let both = pair.old.is_some() && pair.new.is_some();
    egui::Grid::new("binary-facts")
        .num_columns(3)
        .spacing([18.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            for title in [
                crate::i18n::text("repo-core-1564"),
                crate::i18n::text("repo-core-1565"),
            ] {
                ui.label(
                    RichText::new(title)
                        .font(theme::semibold(12.0))
                        .color(t.text_dim),
                );
            }
            ui.end_row();
            for k in &facts {
                let (a, b) = (value(&pair.old, k), value(&pair.new, k));
                let changed = both && a != b;
                ui.label(
                    RichText::new(if k == "repo-core-1693" {
                        crate::i18n::text("repo-core-1693")
                    } else {
                        k
                    })
                    .color(t.text_dim),
                );
                for (v, side, missing, color) in [
                    (&a, &pair.old, crate::i18n::text("repo-core-1695"), t.danger),
                    (
                        &b,
                        &pair.new,
                        crate::i18n::text("repo-core-1696"),
                        t.success,
                    ),
                ] {
                    let shown = match (v, side) {
                        (Some(v), _) => {
                            RichText::new(v).color(if changed { color } else { t.text })
                        }
                        (None, Some(_)) => RichText::new("—").color(t.text_faint),
                        (None, None) => RichText::new(missing).color(t.text_faint),
                    };
                    ui.label(shown);
                }
                ui.end_row();
            }
        });
}

fn text_diff(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let Some(lines) = &pair.text else { return };
    if lines.is_empty() {
        ui.label(RichText::new(crate::i18n::text("repo-core-1697")).color(t.text_dim));
        return;
    }
    if pair.text_changes == 0 {
        let more = if pair.members.is_some() {
            crate::i18n::text("repo-core-1598")
        } else {
            crate::i18n::text("repo-core-1684")
        };
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-core-1698",
                &[("more", format!("{}", more).into())],
            ))
            .color(t.text_dim),
        );
    }
    let font = theme::mono(12.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + 2.0;
    egui::ScrollArea::both()
        .id_salt("binary-text")
        .auto_shrink(false)
        .show_rows(ui, row_h, lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (edit, line) in &lines[range] {
                let (mark, bg, color) = match edit {
                    Edit::Same => (" ", egui::Color32::TRANSPARENT, t.text_faint),
                    Edit::Del => ("−", t.diff_del_bg, t.danger),
                    Edit::Add => ("+", t.diff_add_bg, t.success),
                };
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    mark,
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color,
                        ..Default::default()
                    },
                );
                job.append(" ", 0.0, egui::TextFormat::default());
                job.append(
                    line,
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: t.text,
                        background: bg,
                        ..Default::default()
                    },
                );
                // One row each: a row that wrapped would throw the scrolling off.
                job.wrap.max_width = f32::INFINITY;
                ui.add(egui::Label::new(job).extend());
            }
        });
}

fn members(ui: &mut Ui, t: &Tokens, changes: &[MemberChange]) {
    if changes.is_empty() {
        ui.label(RichText::new(crate::i18n::text("repo-core-1700")).color(t.text_dim));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("binary-members")
        .auto_shrink(false)
        .show_rows(ui, 20.0, changes.len(), |ui, range| {
            for c in &changes[range] {
                ui.horizontal(|ui| {
                    let (word, color, size) = match (c.old, c.new) {
                        (None, Some(n)) => (
                            crate::i18n::text("repo-core-1702"),
                            t.success,
                            human_size(n),
                        ),
                        (Some(o), None) => {
                            (crate::i18n::text("repo-core-1703"), t.danger, human_size(o))
                        }
                        (Some(o), Some(n)) if o == n => (
                            crate::i18n::text("repo-core-1704"),
                            t.warning,
                            crate::i18n::message(
                                "repo-core-1705",
                                &[("n", format!("{}", human_size(n)).into())],
                            ),
                        ),
                        (Some(o), Some(n)) => (
                            crate::i18n::text("repo-core-1704"),
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

/// 16 bytes a row, the sides there one under the other per offset; bytes that differ at the
/// same offset marked.
fn hex(ui: &mut Ui, t: &Tokens, pair: &BinaryPair) {
    let sides: Vec<(&str, &[u8], &[u8], egui::Color32)> = {
        let old = pair.old.as_ref().map(|s| s.head.as_slice());
        let new = pair.new.as_ref().map(|s| s.head.as_slice());
        let mut v = Vec::new();
        if let Some(o) = old {
            v.push(("−", o, new.unwrap_or_default(), t.danger));
        }
        if let Some(n) = new {
            v.push(("+", n, old.unwrap_or_default(), t.success));
        }
        v
    };
    let rows = sides
        .iter()
        .map(|(_, b, _, _)| b.len())
        .max()
        .unwrap_or(0)
        .div_ceil(16);
    let cut = [pair.old.as_ref(), pair.new.as_ref()]
        .into_iter()
        .flatten()
        .any(|s| s.size.is_none_or(|n| n > HEX_BYTES as u64));
    if cut {
        ui.label(
            RichText::new(crate::i18n::text("repo-core-1706"))
                .color(t.text_faint)
                .small(),
        );
    }
    let font = theme::mono(12.0);
    let line_h = ui.fonts_mut(|f| f.row_height(&font));
    let row_h = line_h * sides.len() as f32 + 7.0;
    egui::ScrollArea::both()
        .id_salt("binary-hex")
        .auto_shrink(false)
        .show_rows(ui, row_h, rows, |ui, range| {
            for row in range {
                let at = row * 16;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (mark, bytes, other, color) in &sides {
                        let mut job = egui::text::LayoutJob::default();
                        let plain = egui::TextFormat {
                            font_id: font.clone(),
                            color: t.text_dim,
                            ..Default::default()
                        };
                        job.append(
                            mark,
                            0.0,
                            egui::TextFormat {
                                font_id: font.clone(),
                                color: *color,
                                ..Default::default()
                            },
                        );
                        job.append(&format!(" {at:08x}  "), 0.0, plain.clone());
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
                            let c = if differs { *color } else { t.text };
                            job.append(
                                &text,
                                0.0,
                                egui::TextFormat {
                                    font_id: font.clone(),
                                    color: c,
                                    ..Default::default()
                                },
                            );
                        }
                        job.append(&format!(" {ascii}"), 0.0, plain);
                        ui.add(egui::Label::new(job).extend());
                    }
                    ui.add_space(3.0);
                    let r = ui.available_rect_before_wrap();
                    ui.painter()
                        .hline(r.x_range(), r.top(), egui::Stroke::new(1.0, t.separator));
                    ui.add_space(4.0);
                });
            }
        });
}

/// A line above a diff git made through the file's `textconv` program: the text is that
/// program's; in the working tree (`area`) it is staged, unstaged or discarded as a whole file.
pub(in crate::repo::diff) fn textconv_note(ui: &mut Ui, program: &str, area: Option<Area>) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.chrome)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("repo-core-1711"))
                        .color(t.text_dim)
                        .small(),
                );
                let shown: String = program.chars().take(60).collect();
                let more = if program.chars().count() > 60 {
                    "…"
                } else {
                    ""
                };
                ui.label(
                    RichText::new(format!("{shown}{more}"))
                        .font(theme::mono(11.5))
                        .color(t.text_dim),
                )
                .on_hover_text(program);
                let rest = textconv_rest(area);
                ui.label(RichText::new(rest).color(t.text_dim).small());
            });
        });
}

/// What the textconv line says after the program: what can be done with the file in `area`.
pub(super) fn textconv_rest(area: Option<Area>) -> &'static str {
    match area {
        Some(Area::Staged) => crate::i18n::text("repo-core-1713"),
        Some(Area::Conflicted) => crate::i18n::text("repo-core-1714"),
        Some(_) => crate::i18n::text("repo-core-1715"),
        None => crate::i18n::text("repo-core-1716"),
    }
}
