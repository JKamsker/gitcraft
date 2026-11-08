//! Prose shown: paragraphs wrapped to the pane, edited ones with their removed words struck
//! through on red and their added words on green, removed and added ones whole, and long runs
//! of unchanged paragraphs folded. Only the paragraphs in view are laid out each frame; their
//! heights are measured once per width.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Rect, Stroke, Ui, pos2, vec2};

use super::RichState;
use super::prose::{Part, ProseDiff};
use super::words::{Change, starts_block};
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// The text wraps to this at most, as rendered Markdown does.
const CONTENT_W: f32 = 860.0;
/// Unchanged paragraphs kept in view next to a change; more between two changes fold.
const AROUND: usize = 1;
/// A paragraph longer than this is cut when shown (a text without blank lines is one).
const MAX_CHARS: usize = 20_000;
const MARGIN: f32 = 24.0;
const SPACING: f32 = 6.0;
const FOLD_H: f32 = 22.0;
const CAPTION_H: f32 = 14.0;
/// Padding inside a part, above and below its text.
const PAD_Y: f32 = 8.0;

/// What the whole change is, for a note at the top.
fn summary(prose: &ProseDiff) -> Option<&'static str> {
    let all = |f: fn(&Part) -> bool| !prose.parts.is_empty() && prose.parts.iter().all(f);
    if prose.parts.is_empty() {
        Some(crate::i18n::text("repo-core-2205"))
    } else if all(|p| matches!(p, Part::Added { .. })) {
        Some(crate::i18n::text("repo-core-2206"))
    } else if all(|p| matches!(p, Part::Removed { .. })) {
        Some(crate::i18n::text("repo-core-2207"))
    } else if all(|p| {
        matches!(
            p,
            Part::Kept {
                rewrapped: false,
                ..
            }
        )
    }) {
        Some(
            crate::i18n::text("repo-core-2208"),
        )
    } else if all(|p| matches!(p, Part::Kept { .. })) {
        Some(crate::i18n::text("repo-core-2209"))
    } else {
        None
    }
}

pub(super) fn show(ui: &mut Ui, prose: &ProseDiff, state: &mut RichState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let width = (ui.available_width() - 2.0 * MARGIN).clamp(200.0, CONTENT_W);
    let note = summary(prose);
    // A file all new or all gone needs no caption on each paragraph.
    let captions = !matches!(note, Some(n) if n.starts_with("The whole") || n.contains("deletes"));
    let items = layout(&prose.parts, &state.opened);
    if state.heights.len() != items.len() || state.width != width {
        state.width = width;
        state.heights = items
            .iter()
            .map(|item| match item {
                Item::Fold { .. } => FOLD_H,
                Item::Part(i) => {
                    let job = part_job(&prose.parts[*i], &t, width);
                    let h = ui.fonts_mut(|f| f.layout_job(job)).size().y;
                    h + PAD_Y
                        + if caption(&prose.parts[*i], captions).is_some() {
                            CAPTION_H
                        } else {
                            0.0
                        }
                }
            })
            .collect();
    }
    let note_h = if note.is_some() { 28.0 } else { 0.0 };
    let total: f32 = state.heights.iter().map(|h| h + SPACING).sum::<f32>() + note_h + 2.0 * 16.0;
    egui::ScrollArea::vertical()
        .id_salt("rich_prose")
        .auto_shrink(false)
        .show_viewport(ui, |ui, viewport| {
            let (area, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), total), egui::Sense::hover());
            let left = area.left() + MARGIN;
            let mut y = area.top() + 16.0;
            if let Some(note) = note {
                ui.painter().text(
                    pos2(left, y),
                    egui::Align2::LEFT_TOP,
                    note,
                    egui::FontId::proportional(13.0),
                    t.text_dim,
                );
                y += note_h;
            }
            let top = area.top() + viewport.top();
            let bottom = area.top() + viewport.bottom();
            for (k, item) in items.iter().enumerate() {
                let h = state.heights.get(k).copied().unwrap_or(FOLD_H);
                let rect = Rect::from_min_size(pos2(left, y), vec2(width, h));
                y += h + SPACING;
                if rect.bottom() < top || rect.top() > bottom {
                    continue;
                }
                match *item {
                    Item::Part(i) => {
                        part(ui, &t, &prose.parts[i], rect, captions, acts);
                    }
                    Item::Fold { run, count } => {
                        if fold(ui, &t, rect, count, run).clicked() {
                            state.opened.insert(run);
                        }
                    }
                }
            }
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
fn fold(ui: &mut Ui, t: &Tokens, rect: Rect, count: usize, run: usize) -> egui::Response {
    let label = if count == 1 {
        crate::i18n::text("repo-core-2213").to_owned()
    } else {
        crate::i18n::message("repo-core-2214", &[("count", (count).into())])
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label, theme::medium(11.5), t.text_dim);
    let r = Rect::from_min_size(rect.min, vec2(galley.size().x + 16.0, rect.height()));
    let resp = ui
        .interact(r, ui.id().with(("prose_fold", run)), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(crate::i18n::text("repo-core-2216"));
    let fill = if resp.hovered() { t.hover } else { t.card };
    ui.painter().rect_filled(r, t.radius_sm, fill);
    ui.painter().galley(
        pos2(r.left() + 8.0, r.center().y - galley.size().y / 2.0),
        galley,
        t.text_dim,
    );
    resp
}

/// The caption under a part, if it has one.
fn caption(part: &Part, captions: bool) -> Option<&'static str> {
    match part {
        Part::Kept {
            rewrapped: true, ..
        } => Some(crate::i18n::text("repo-core-2217")),
        Part::Kept { .. } => None,
        Part::Changed { .. } => Some(crate::i18n::text("repo-core-2218")),
        Part::Removed { .. } => captions.then_some(crate::i18n::text("repo-core-1703")),
        Part::Added { .. } => captions.then_some(crate::i18n::text("repo-core-1702")),
    }
}

/// The text of a part, laid out to wrap within `width`.
pub(super) fn part_job(part: &Part, t: &Tokens, width: f32) -> LayoutJob {
    let font = egui::FontId::proportional(14.0);
    let mut job = match part {
        Part::Kept { text, .. } => plain(text, &font, t.text_dim, false),
        Part::Removed { text, .. } => plain(text, &font, t.text, true),
        Part::Added { text, .. } => plain(text, &font, t.text, false),
        Part::Changed { words, .. } => changed(words, &font, t),
    };
    job.wrap.max_width = width - 18.0;
    job
}

/// One part in `rect`: wrapped, on its colour with a bar on its left; a click shows it in the
/// diff.
fn part(ui: &mut Ui, t: &Tokens, part: &Part, rect: Rect, captions: bool, acts: &mut Vec<Action>) {
    let (fill, bar, old, new) = match part {
        Part::Kept { old, new, .. } => (Color32::TRANSPARENT, None, Some(*old), Some(*new)),
        Part::Removed { old, .. } => (t.diff_del_bg, Some(t.danger), Some(*old), None),
        Part::Added { new, .. } => (t.diff_add_bg, Some(t.success), None, Some(*new)),
        Part::Changed { old, new, .. } => (t.card, Some(t.accent), Some(*old), Some(*new)),
    };
    let resp = ui
        .interact(
            rect,
            ui.id().with(("prose_part", old, new)),
            egui::Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    p.rect_filled(
        rect,
        t.radius_sm,
        if resp.hovered() && fill == Color32::TRANSPARENT {
            t.hover
        } else {
            fill
        },
    );
    if let Some(bar) = bar {
        p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0, bar));
    }
    let galley = ui.fonts_mut(|f| f.layout_job(part_job(part, t, rect.width())));
    let text_h = galley.size().y;
    ui.painter().galley(
        pos2(rect.left() + 12.0, rect.top() + PAD_Y / 2.0),
        galley,
        t.text,
    );
    if let Some(caption) = caption(part, captions) {
        ui.painter().text(
            pos2(rect.left() + 12.0, rect.top() + PAD_Y / 2.0 + text_h),
            egui::Align2::LEFT_TOP,
            caption,
            egui::FontId::proportional(10.5),
            t.text_faint,
        );
    }
    let tip = match (old, new) {
        (Some(o), None) => crate::i18n::message("repo-core-2220", &[("o", format!("{}", o).into())]),
        (_, Some(n)) => crate::i18n::message("repo-core-2221", &[("n", (n).into())]),
        _ => String::new(),
    };
    let label = match part {
        Part::Kept { text, .. } | Part::Removed { text, .. } | Part::Added { text, .. } => {
            text.chars().take(200).collect()
        }
        Part::Changed { words, .. } => words.iter().map(|(_, w)| w.as_str()).collect::<String>(),
    };
    crate::a11y::a11y(&resp, egui::WidgetType::Button, &label);
    if resp.on_hover_text(tip).clicked() {
        acts.push(Action::DiffGoToLine { old, new });
    }
}

/// A paragraph's text as one flow, a line break kept before a list item or heading; cut when
/// very long.
fn flow(text: &str) -> String {
    let mut out = String::new();
    for (k, line) in text.lines().enumerate() {
        if k > 0 {
            out.push(if starts_block(line) { '\n' } else { ' ' });
        }
        out.push_str(line.trim());
    }
    cut(out)
}

fn cut(text: String) -> String {
    let total = text.chars().count();
    if total <= MAX_CHARS {
        return text;
    }
    let mut s: String = text.chars().take(MAX_CHARS).collect();
    s.push_str(&crate::i18n::message("repo-core-2222", &[("MAX-CHARS", format!("{}", total - MAX_CHARS).into())]));
    s
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

/// An edited paragraph's words, each with what stood before it: kept ones plain, removed ones
/// struck through on red, added ones on green.
fn changed(words: &[(Change, String)], font: &egui::FontId, t: &Tokens) -> LayoutJob {
    changed_with_limit(words, font, t, Some(MAX_CHARS))
}
pub(super) fn search_changed(words: &[(Change, String)], t: &Tokens) -> LayoutJob {
    changed_with_limit(words, &egui::FontId::proportional(14.0), t, None)
}
fn append(job: &mut LayoutJob, text: &str, format: TextFormat) {
    if let Some(last) = job.sections.last_mut()
        && last.format == format
        && last.byte_range.end.0 == job.text.len()
    {
        job.text.push_str(text);
        last.byte_range.end = egui::text::ByteIndex(job.text.len());
    } else {
        job.append(text, 0.0, format);
    }
}
fn changed_with_limit(
    words: &[(Change, String)],
    font: &egui::FontId,
    t: &Tokens,
    limit: Option<usize>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut shown = 0;
    for (change, word) in words {
        if limit.is_some_and(|limit| shown > limit) {
            job.append(" …", 0.0, TextFormat::simple(font.clone(), t.text_faint));
            break;
        }
        shown += word.len();
        // The space before a word stays plain; only the word is marked.
        let lead = word.len() - word.trim_start().len();
        let (space, body) = word.split_at(lead);
        if !space.is_empty() {
            append(&mut job, space, TextFormat::simple(font.clone(), t.text));
        }
        let (background, strike) = match change {
            Change::Kept => (Color32::TRANSPARENT, Stroke::NONE),
            Change::Removed => (t.danger.gamma_multiply(0.35), Stroke::new(1.0, t.text_dim)),
            Change::Added => (t.success.gamma_multiply(0.35), Stroke::NONE),
        };
        append(
            &mut job,
            body,
            TextFormat {
                font_id: font.clone(),
                color: t.text,
                background,
                strikethrough: strike,
                ..Default::default()
            },
        );
    }
    job
}
