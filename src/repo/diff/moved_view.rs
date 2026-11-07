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
const ROW_H: f32 = 19.0;

/// One line of a block's comparison: its mark (` `: kept, `−`: only where it was, `+`: only
/// where it is now), the line (its index in the diff), and its changed chars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct CompareLine {
    mark: char,
    line: usize,
    changed: Option<(usize, usize)>,
}

/// The moved block whose two places are shown together, compared once when it is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Opened {
    /// Its index among the diff's moves.
    pub(in crate::repo) block: usize,
    rows: Vec<CompareLine>,
    /// Lines edited on the way (a line changed in place counts once).
    edited: usize,
    /// Unchanged but for the whitespace around its lines.
    reindented: bool,
}

impl Opened {
    /// Block `b` of `data` compared, if it is there.
    pub(in crate::repo) fn new(data: &DiffData, b: usize) -> Option<Self> {
        let block = data.moves.blocks.get(b)?;
        let lines = &data.lines;
        let compared = moved::compare(lines, block);
        let mut rows = Vec::with_capacity(compared.len());
        let mut edited = 0;
        let mut reindented = false;
        let mut k = 0;
        while k < compared.len() {
            if let (Some(i), Some(j)) = compared[k] {
                reindented |= lines[i].raw.get(1..) != lines[j].raw.get(1..);
                rows.push(CompareLine {
                    mark: ' ',
                    line: i,
                    changed: None,
                });
                k += 1;
                continue;
            }
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
            edited += olds.len().max(news.len());
            let pair = |o: usize, n: usize| changed_ranges(&lines[o].text, &lines[n].text);
            for (x, &o) in olds.iter().enumerate() {
                let changed = news.get(x).and_then(|&n| pair(o, n)).map(|r| r.0);
                rows.push(CompareLine {
                    mark: '−',
                    line: o,
                    changed,
                });
            }
            for (x, &n) in news.iter().enumerate() {
                let changed = olds.get(x).and_then(|&o| pair(o, n)).map(|r| r.1);
                rows.push(CompareLine {
                    mark: '+',
                    line: n,
                    changed,
                });
            }
        }
        Some(Self {
            block: b,
            rows,
            edited,
            reindented,
        })
    }

    /// How many lines were edited on the way.
    #[cfg(test)]
    pub(in crate::repo) fn edited_lines(&self) -> usize {
        self.edited
    }

    /// Where the block is: the number of its first line where it was and where it is now.
    pub(in crate::repo) fn place(data: &DiffData, b: usize) -> Option<(u32, u32)> {
        let block = data.moves.blocks.get(b)?;
        Some((
            data.lines.get(block.del.start)?.old?,
            data.lines.get(block.add.start)?.new?,
        ))
    }
}

/// The panel comparing the moved block `open` of `data` where it was with where it is now.
pub(super) fn panel(
    ui: &mut Ui,
    t: &Tokens,
    data: &DiffData,
    open: &Opened,
    acts: &mut Vec<Action>,
) {
    let Some(block) = data.moves.blocks.get(open.block) else {
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
            header(ui, t, &data.lines, block, open, acts);
            ui.add_space(6.0);
            content(ui, t, data, open);
        });
}

/// Read-only moved content, also used by the change-wide source/destination view.
pub(in crate::repo) fn content(ui: &mut Ui, t: &Tokens, data: &DiffData, open: &Opened) {
    let font = theme::mono(12.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::vertical()
        .id_salt(("diff_moved_rows", open.block))
        .max_height(MAX_H)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_H, open.rows.len(), |ui, range| {
            for r in &open.rows[range] {
                let text = data.lines.get(r.line).map_or("", |l| l.text.as_str());
                row(ui, t, &font, r.mark, text, r.changed);
            }
        });
}

/// Where the block was and is, how much changed, and the buttons to go to either place.
fn header(
    ui: &mut Ui,
    t: &Tokens,
    lines: &[DiffLine],
    block: &MovedBlock,
    open: &Opened,
    acts: &mut Vec<Action>,
) {
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
    let edited = open.edited;
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
            0 if open.reindented => "only re-indented on the way".to_owned(),
            0 => "unchanged on the way".to_owned(),
            1 => "1 line edited on the way".to_owned(),
            n => format!("{n} lines edited on the way"),
        };
        ui.label(egui::RichText::new(note).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
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

/// One line of the comparison: its mark, its text with `changed` chars marked.
fn row(
    ui: &mut Ui,
    t: &Tokens,
    font: &egui::FontId,
    mark: char,
    text: &str,
    changed: Option<(usize, usize)>,
) {
    let fill = match mark {
        '+' => t.diff_add_bg,
        '−' => t.diff_del_bg,
        _ => moved_bg(t),
    };
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
        .inner_margin(egui::Margin::symmetric(6, 0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_height(ROW_H);
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
