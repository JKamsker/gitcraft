//! _Moved to…_: the two places of a moved block together, below the diff, with the lines
//! edited on the way marked.

use egui::{Color32, Stroke, Ui};

use super::moved::{self, MovedBlock};
use super::paint::{line_job, moved_bg};
use super::parse::changed_ranges;
use super::{DiffData, DiffLine};
use crate::icons;
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// How tall the comparison gets before it scrolls.
const MAX_H: f32 = 240.0;

/// The panel comparing moved block `b` of `data` where it was with where it is now.
pub(super) fn panel(ui: &mut Ui, t: &Tokens, data: &DiffData, b: usize, acts: &mut Vec<Action>) {
    let Some(block) = data.moves.blocks.get(b) else {
        return;
    };
    egui::Panel::bottom("diff_moved_block")
        .frame(
            egui::Frame::new()
                .fill(t.card)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            header(ui, t, &data.lines, block, acts);
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt(("diff_moved_rows", b))
                .max_height(MAX_H)
                .auto_shrink([false, true])
                .show(ui, |ui| rows(ui, t, &data.lines, block));
        });
}

/// Where the block was and is, how much changed, and the buttons to go to either place.
fn header(ui: &mut Ui, t: &Tokens, lines: &[DiffLine], block: &MovedBlock, acts: &mut Vec<Action>) {
    let span = |range: &std::ops::Range<usize>, old: bool| {
        let n = |i: usize| {
            lines
                .get(i)
                .and_then(|l| if old { l.old } else { l.new })
                .unwrap_or(0)
        };
        let (a, z) = (n(range.start), n(range.end.saturating_sub(1)));
        if a == z {
            format!("line {a}")
        } else {
            format!("lines {a}–{z}")
        }
    };
    let edited = moved::compare(lines, block)
        .iter()
        .filter(|(o, n)| o.is_none() || n.is_none())
        .count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "Moved from {} to {}",
                span(&block.del, true),
                span(&block.add, false)
            ))
            .font(theme::medium(12.5))
            .color(t.text),
        );
        let note = match edited {
            0 => "unchanged on the way".to_owned(),
            1 => "1 line edited on the way".to_owned(),
            n => format!("{n} lines edited on the way"),
        };
        ui.label(egui::RichText::new(note).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close").clicked() {
                acts.push(Action::DiffMoved(None));
            }
            if ui
                .button("Go to where it is now")
                .on_hover_text("Scroll the diff to the added block")
                .clicked()
            {
                acts.push(Action::DiffJumpLine(block.add.start));
            }
            if ui
                .button("Go to where it was")
                .on_hover_text("Scroll the diff to the removed block")
                .clicked()
            {
                acts.push(Action::DiffJumpLine(block.del.start));
            }
        });
    });
}

/// The block's lines: those it kept faint, those edited on the way red where it was and green
/// where it is, with the changed part of a line edited in place marked.
fn rows(ui: &mut Ui, t: &Tokens, lines: &[DiffLine], block: &MovedBlock) {
    let font = theme::mono(12.0);
    let compared = moved::compare(lines, block);
    let mut k = 0;
    while k < compared.len() {
        match compared[k] {
            (Some(i), Some(_)) => {
                row(ui, t, &font, ' ', &lines[i].text, None, moved_bg(t));
                k += 1;
            }
            _ => {
                // A run of lines only where it was, then only where it is now: paired up.
                let start = k;
                while k < compared.len() && compared[k].1.is_none() {
                    k += 1;
                }
                let mid = k;
                while k < compared.len() && compared[k].0.is_none() {
                    k += 1;
                }
                let olds: Vec<usize> = compared[start..mid].iter().filter_map(|r| r.0).collect();
                let news: Vec<usize> = compared[mid..k].iter().filter_map(|r| r.1).collect();
                let pair = |o: usize, n: usize| changed_ranges(&lines[o].text, &lines[n].text);
                for (x, &o) in olds.iter().enumerate() {
                    let ranges = news.get(x).and_then(|&n| pair(o, n)).map(|r| r.0);
                    row(ui, t, &font, '−', &lines[o].text, ranges, t.diff_del_bg);
                }
                for (x, &n) in news.iter().enumerate() {
                    let ranges = olds.get(x).and_then(|&o| pair(o, n)).map(|r| r.1);
                    row(ui, t, &font, '+', &lines[n].text, ranges, t.diff_add_bg);
                }
            }
        }
    }
}

/// One line of the comparison: its mark, its text with `changed` chars marked, on `fill`.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    t: &Tokens,
    font: &egui::FontId,
    mark: char,
    text: &str,
    changed: Option<(usize, usize)>,
    fill: Color32,
) {
    let strong = match mark {
        '+' => t.success.gamma_multiply(0.35),
        '−' => t.danger.gamma_multiply(0.35),
        _ => Color32::TRANSPARENT,
    };
    let color = if mark == ' ' { t.text_dim } else { t.text };
    let mut job = line_job(&[(color, text)], changed, font, strong);
    job.wrap.max_width = f32::INFINITY;
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let mark_color = match mark {
                    '+' => t.success,
                    '−' => t.danger,
                    _ => t.text_faint,
                };
                ui.label(
                    egui::RichText::new(mark.to_string())
                        .font(font.clone())
                        .color(mark_color),
                );
                ui.label(job);
            });
        });
}
