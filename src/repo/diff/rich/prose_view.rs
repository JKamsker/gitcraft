//! Prose shown: paragraphs wrapped to the pane, edited ones with their removed words struck
//! through on red and their added words on green, removed and added ones whole, and long runs
//! of unchanged paragraphs folded.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Stroke, Ui};

use super::RichState;
use super::prose::{Part, ProseDiff};
use super::words::{Change, spaced};
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// The text wraps to this at most, as rendered Markdown does.
const CONTENT_W: f32 = 860.0;
/// Unchanged paragraphs kept in view next to a change; more between two changes fold.
const AROUND: usize = 1;

pub(super) fn show(ui: &mut Ui, prose: &ProseDiff, state: &mut RichState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let width = (ui.available_width() - 48.0).clamp(200.0, CONTENT_W);
    egui::ScrollArea::vertical()
        .id_salt("rich_prose")
        .auto_shrink(false)
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    ui.set_max_width(width);
                    if prose.parts.iter().all(|p| matches!(p, Part::Kept { .. })) {
                        let note = if prose.parts.iter().any(|p| {
                            matches!(
                                p,
                                Part::Kept {
                                    rewrapped: true,
                                    ..
                                }
                            )
                        }) {
                            "The words are as they were: only how the lines are wrapped changed."
                        } else {
                            "The words are as they were: only whitespace or blank lines changed."
                        };
                        ui.label(egui::RichText::new(note).color(t.text_dim).italics());
                        ui.add_space(12.0);
                    }
                    for item in layout(&prose.parts, &state.opened) {
                        match item {
                            Item::Part(i) => part(ui, &t, &prose.parts[i], width, acts),
                            Item::Fold { run, count } => {
                                if fold(ui, &t, count).clicked() {
                                    state.opened.insert(run);
                                }
                            }
                        }
                        ui.add_space(6.0);
                    }
                });
        });
}

/// What the view shows, in order: parts, and folds of unchanged ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Part(usize),
    /// `count` unchanged paragraphs folded; `run` names the fold (its first part).
    Fold {
        run: usize,
        count: usize,
    },
}

/// The parts shown and folded: in each run of unchanged ones longer than what stays in view
/// around the changes (and not opened), the middle folds.
pub(super) fn layout(parts: &[Part], opened: &std::collections::HashSet<usize>) -> Vec<Item> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if !matches!(parts[i], Part::Kept { .. }) {
            out.push(Item::Part(i));
            i += 1;
            continue;
        }
        let start = i;
        while i < parts.len() && matches!(parts[i], Part::Kept { .. }) {
            i += 1;
        }
        let keep_top = if start == 0 { 0 } else { AROUND };
        let keep_bottom = if i == parts.len() { 0 } else { AROUND };
        let len = i - start;
        let all_kept = start == 0 && i == parts.len();
        if opened.contains(&start) || all_kept || len <= keep_top + keep_bottom + 1 {
            out.extend((start..i).map(Item::Part));
            continue;
        }
        out.extend((start..start + keep_top).map(Item::Part));
        out.push(Item::Fold {
            run: start,
            count: len - keep_top - keep_bottom,
        });
        out.extend((i - keep_bottom..i).map(Item::Part));
    }
    out
}

/// A fold of unchanged paragraphs; clicked, it opens.
fn fold(ui: &mut Ui, t: &Tokens, count: usize) -> egui::Response {
    let label = if count == 1 {
        "1 unchanged paragraph".to_owned()
    } else {
        format!("{count} unchanged paragraphs")
    };
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .color(t.text_dim)
                .font(theme::medium(11.5)),
        )
        .fill(t.card),
    )
    .on_hover_text("Show them")
}

/// One part: wrapped, on its colour with a bar on its left; a click shows it in the diff.
fn part(ui: &mut Ui, t: &Tokens, part: &Part, width: f32, acts: &mut Vec<Action>) {
    let font = egui::FontId::proportional(14.0);
    let (fill, bar, job, (old, new), note) = match part {
        Part::Kept {
            old,
            new,
            text,
            rewrapped,
        } => (
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            plain(text, &font, t.text_dim, false),
            (Some(*old), Some(*new)),
            rewrapped.then_some("re-wrapped"),
        ),
        Part::Removed { old, text } => (
            t.diff_del_bg,
            t.danger,
            plain(text, &font, t.text, true),
            (Some(*old), None),
            Some("removed"),
        ),
        Part::Added { new, text } => (
            t.diff_add_bg,
            t.success,
            plain(text, &font, t.text, false),
            (None, Some(*new)),
            Some("added"),
        ),
        Part::Changed { old, new, words } => (
            t.card,
            t.accent,
            changed(words, &font, t),
            (Some(*old), Some(*new)),
            None,
        ),
    };
    let mut job = job;
    job.wrap.max_width = width - 18.0;
    let frame = egui::Frame::new()
        .fill(fill)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin {
            left: 12,
            right: 6,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(width - 18.0);
            ui.add(egui::Label::new(job).selectable(false));
            if let Some(note) = note {
                ui.label(egui::RichText::new(note).small().color(t.text_faint));
            }
        });
    let rect = frame.response.rect;
    if bar != Color32::TRANSPARENT {
        ui.painter()
            .line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0, bar));
    }
    let line = new.or(old).unwrap_or(0);
    let resp = ui
        .interact(
            rect,
            ui.id().with(("prose_part", old, new)),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Line {line}: click to see it in the diff"));
    if resp.clicked() {
        acts.push(Action::DiffGoToLine { old, new });
    }
}

/// A paragraph's text as one flow, a line break kept before a list item or heading.
fn flow(text: &str) -> String {
    let mut out = String::new();
    for (k, line) in text.lines().enumerate() {
        if k > 0 {
            let t = line.trim_start();
            let starts_block = t.starts_with(['-', '*', '+', '#', '|', '>', '.'])
                || t.split_once(['.', ')'])
                    .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            out.push(if starts_block { '\n' } else { ' ' });
        }
        out.push_str(line.trim());
    }
    out
}

fn plain(text: &str, font: &egui::FontId, color: Color32, struck: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &flow(text),
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            strikethrough: if struck {
                Stroke::new(1.0, color.gamma_multiply(0.7))
            } else {
                Stroke::NONE
            },
            ..Default::default()
        },
    );
    job
}

/// An edited paragraph's words: kept ones plain, removed ones struck through on red, added
/// ones on green.
fn changed(words: &[(Change, String)], font: &egui::FontId, t: &Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut prev: Option<&str> = None;
    for (change, word) in words {
        if let Some(p) = prev
            && spaced(p, word)
        {
            job.append(" ", 0.0, TextFormat::simple(font.clone(), t.text));
        }
        let (background, strike) = match change {
            Change::Kept => (Color32::TRANSPARENT, Stroke::NONE),
            Change::Removed => (t.danger.gamma_multiply(0.35), Stroke::new(1.0, t.text_dim)),
            Change::Added => (t.success.gamma_multiply(0.35), Stroke::NONE),
        };
        job.append(
            word,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: t.text,
                background,
                strikethrough: strike,
                ..Default::default()
            },
        );
        prev = Some(word);
    }
    job
}
