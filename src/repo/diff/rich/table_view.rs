//! Tables shown as a grid: the columns (added, removed, renamed marked) in a header that stays
//! on top, the rows added, removed and changed (with the changed cells as `old → new`), and
//! the unchanged rows between them folded. A click on a row goes to its line.

use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::RichState;
use super::table::{Change, Column, Row, TableDiff};
use crate::repo::Action;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 22.0;
/// A column is as wide as its longest value, within these (in characters).
const MIN_CHARS: usize = 6;
const MAX_CHARS: usize = 48;
/// Unchanged rows kept in view next to a change.
const AROUND: usize = 1;
const NUM_W: f32 = 54.0;

/// What the grid shows, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Row(usize),
    /// Unchanged rows folded; the first of them names the fold.
    Fold {
        first: usize,
        count: usize,
    },
}

/// The rows shown and folded: unchanged runs keep a row next to each change.
pub(super) fn layout(rows: &[Row], opened: &std::collections::HashSet<usize>) -> Vec<Item> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if rows[i].change != Change::Kept {
            out.push(Item::Row(i));
            i += 1;
            continue;
        }
        let start = i;
        while i < rows.len() && rows[i].change == Change::Kept {
            i += 1;
        }
        let top = if start == 0 { 0 } else { AROUND };
        let bottom = if i == rows.len() { 0 } else { AROUND };
        if opened.contains(&start) || i - start <= top + bottom + 1 {
            out.extend((start..i).map(Item::Row));
            continue;
        }
        out.extend((start..start + top).map(Item::Row));
        out.push(Item::Fold {
            first: start,
            count: i - start - top - bottom,
        });
        out.extend((i - bottom..i).map(Item::Row));
    }
    out
}

pub(super) fn show(ui: &mut Ui, diff: &TableDiff, state: &mut RichState, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let count = |c: Change| diff.rows.iter().filter(|r| r.change == c).count();
    let renamed = diff
        .columns
        .iter()
        .filter(|c| c.change == Change::Changed)
        .count();
    let col_count = |c: Change| diff.columns.iter().filter(|x| x.change == c).count();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-core-2246",
                        &[("format", diff.format.to_string().into())],
                    ))
                    .font(theme::medium(12.5))
                    .color(t.text),
                );
                let parts: Vec<String> = [
                    (count(Change::Changed), "repo-core-table-rows-changed"),
                    (count(Change::Added), "repo-core-table-rows-added"),
                    (count(Change::Removed), "repo-core-table-rows-removed"),
                    (col_count(Change::Added), "repo-core-table-columns-added"),
                    (
                        col_count(Change::Removed),
                        "repo-core-table-columns-removed",
                    ),
                    (renamed, "repo-core-table-columns-renamed"),
                ]
                .iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, id)| crate::i18n::message(id, &[("count", (*n).into())]))
                .collect();
                let summary = if parts.is_empty() {
                    crate::i18n::text("repo-core-2251").to_owned()
                } else {
                    parts.join(" · ")
                };
                ui.label(RichText::new(summary).color(t.text_dim));
                if diff.keyed {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-core-2252")).color(t.text_faint),
                    )
                    .on_hover_text(crate::i18n::text("repo-core-2253"));
                }
            });
        });
    let font = theme::mono(12.0);
    let char_w = ui
        .painter()
        .layout_no_wrap("M".to_owned(), font.clone(), t.text)
        .size()
        .x;
    let widths: Vec<f32> = diff
        .widths
        .iter()
        .map(|w| (*w).clamp(MIN_CHARS, MAX_CHARS) as f32 * char_w + 16.0)
        .collect();
    let total_w = NUM_W + widths.iter().sum::<f32>();
    let items = layout(&diff.rows, &state.opened);
    egui::ScrollArea::both()
        .id_salt("rich_table")
        .auto_shrink(false)
        .show_viewport(ui, |ui, viewport| {
            let (area, _) = ui.allocate_exact_size(
                vec2(
                    total_w.max(ui.available_width()),
                    (items.len() + 1) as f32 * ROW_H,
                ),
                Sense::hover(),
            );
            let first = (viewport.top() / ROW_H).floor().max(0.0) as usize;
            let last = ((viewport.bottom() / ROW_H).ceil() as usize).min(items.len());
            for (k, item) in items.iter().enumerate().take(last).skip(first) {
                let top = area.top() + (k + 1) as f32 * ROW_H;
                let rect = Rect::from_min_size(
                    pos2(area.left(), top),
                    vec2(total_w.max(area.width()), ROW_H),
                );
                match *item {
                    Item::Row(i) => row(
                        ui,
                        &t,
                        &font,
                        &widths,
                        &diff.rows[i],
                        &diff.columns,
                        rect,
                        i,
                        acts,
                    ),
                    Item::Fold { first, count } => {
                        let label = if count == 1 {
                            crate::i18n::text("repo-core-2255").to_owned()
                        } else {
                            crate::i18n::message("repo-core-2256", &[("count", (count).into())])
                        };
                        let resp = ui
                            .interact(rect, ui.id().with(("table_fold", first)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(crate::i18n::text("repo-core-2216"));
                        crate::a11y::expanded(
                            &resp,
                            || {
                                crate::i18n::message(
                                    "repo-core-2258",
                                    &[(
                                        "label",
                                        crate::repo::messages::display_operation(&label).into(),
                                    )],
                                )
                            },
                            false,
                        );
                        ui.painter().rect_filled(
                            rect,
                            0.0,
                            if resp.hovered() { t.hover } else { t.card },
                        );
                        ui.painter().text(
                            pos2(rect.left() + NUM_W, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            label,
                            theme::medium(11.5),
                            t.text_dim,
                        );
                        if resp.clicked() {
                            state.opened.insert(first);
                        }
                    }
                }
            }
            // The header stays on top as the rows scroll under it.
            let head = Rect::from_min_size(
                pos2(area.left(), area.top() + viewport.top()),
                vec2(total_w.max(area.width()), ROW_H),
            );
            header(ui, &t, &widths, diff, head);
        });
}

fn header(ui: &mut Ui, t: &Tokens, widths: &[f32], diff: &TableDiff, rect: Rect) {
    // Clicks on the header don't reach the row scrolled under it.
    ui.interact(rect, ui.id().with("table_head"), Sense::CLICK);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, t.chrome);
    p.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let mut x = rect.left() + NUM_W;
    for (c, w) in widths.iter().enumerate() {
        let (color, fill, tip) = match diff.columns[c].change {
            Change::Added => (
                t.success,
                t.diff_add_bg,
                crate::i18n::text("repo-core-2260"),
            ),
            Change::Removed => (t.danger, t.diff_del_bg, crate::i18n::text("repo-core-2261")),
            Change::Changed => (
                t.accent,
                Color32::TRANSPARENT,
                crate::i18n::text("repo-core-2262"),
            ),
            Change::Kept => (t.text, Color32::TRANSPARENT, ""),
        };
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, ROW_H));
        p.rect_filled(cell, 0.0, fill);
        p.with_clip_rect(cell.shrink(2.0).intersect(ui.clip_rect()))
            .text(
                pos2(x + 8.0, rect.center().y),
                egui::Align2::LEFT_CENTER,
                diff.columns[c].name(),
                theme::medium(12.0),
                color,
            );
        if !tip.is_empty() {
            ui.interact(cell, ui.id().with(("table_head", c)), Sense::hover())
                .on_hover_text(tip);
        }
        x += w;
    }
}

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    t: &Tokens,
    font: &egui::FontId,
    widths: &[f32],
    r: &Row,
    cols: &[Column],
    rect: Rect,
    i: usize,
    acts: &mut Vec<Action>,
) {
    let (fill, mark) = match r.change {
        Change::Added => (t.diff_add_bg, "+"),
        Change::Removed => (t.diff_del_bg, "−"),
        Change::Changed => (Color32::TRANSPARENT, "~"),
        Change::Kept => (Color32::TRANSPARENT, ""),
    };
    let resp = ui
        .interact(rect, ui.id().with(("table_row", i)), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, if resp.hovered() { t.hover } else { fill });
    let line = r.new_line.or(r.old_line);
    p.text(
        pos2(rect.left() + NUM_W - 8.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        format!("{mark} {}", line.map(|l| l.to_string()).unwrap_or_default()),
        theme::mono(11.0),
        t.text_faint,
    );
    let mut x = rect.left() + NUM_W;
    let mut tip = String::new();
    for (c, w) in widths.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, ROW_H));
        let old = r
            .old
            .as_ref()
            .and_then(|v| v.get(c))
            .map_or("", String::as_str);
        let new = r
            .new
            .as_ref()
            .and_then(|v| v.get(c))
            .map_or("", String::as_str);
        let mut job = egui::text::LayoutJob::default();
        let fmt = |color: Color32, bg: Color32, strike: bool| egui::text::TextFormat {
            font_id: font.clone(),
            color,
            background: bg,
            strikethrough: if strike {
                Stroke::new(1.0, t.text_dim)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        };
        let shown = |v: &str| v.replace(['\n', '\r'], " ");
        let column = cols.get(c).map_or(Change::Kept, |c| c.change);
        let shared = cols.get(c).is_some_and(Column::shared);
        match (r.change, column) {
            // A column only on one side: its value, not a change of it.
            (_, Change::Added) => job.append(
                &shown(new),
                0.0,
                fmt(t.success, Color32::TRANSPARENT, false),
            ),
            (_, Change::Removed) => {
                job.append(&shown(old), 0.0, fmt(t.danger, Color32::TRANSPARENT, false))
            }
            (Change::Changed, _) if shared && old != new => {
                job.append(
                    &shown(old),
                    0.0,
                    fmt(t.text, t.danger.gamma_multiply(0.3), true),
                );
                job.append(" → ", 0.0, fmt(t.text_faint, Color32::TRANSPARENT, false));
                job.append(
                    &shown(new),
                    0.0,
                    fmt(t.text, t.success.gamma_multiply(0.3), false),
                );
                let name = cols.get(c).map(Column::name).unwrap_or_default();
                tip.push_str(&format!("{name}: {old} → {new}\n"));
            }
            (Change::Removed, _) => {
                job.append(&shown(old), 0.0, fmt(t.text, Color32::TRANSPARENT, false))
            }
            (Change::Kept, _) => job.append(
                &shown(new),
                0.0,
                fmt(t.text_dim, Color32::TRANSPARENT, false),
            ),
            _ => job.append(&shown(new), 0.0, fmt(t.text, Color32::TRANSPARENT, false)),
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        p.with_clip_rect(cell.shrink2(vec2(4.0, 0.0)).intersect(ui.clip_rect()))
            .galley(
                pos2(x + 8.0, rect.center().y - galley.size().y / 2.0),
                galley,
                t.text,
            );
        p.line_segment(
            [cell.right_top(), cell.right_bottom()],
            Stroke::new(1.0, t.separator.gamma_multiply(0.5)),
        );
        x += w;
    }
    let label: String = r
        .new
        .as_ref()
        .or(r.old.as_ref())
        .map(|v| v.join(", "))
        .unwrap_or_default();
    crate::a11y::a11y(&resp, egui::WidgetType::Button, &label);
    let where_ = match (r.change, r.old_line, r.new_line) {
        (Change::Removed, Some(o), _) => {
            crate::i18n::message("repo-core-table-old-line", &[("line", o.into())])
        }
        (_, _, Some(n)) => crate::i18n::message("repo-core-2039", &[("n", n.into())]),
        _ => String::new(),
    };
    let tip = if tip.is_empty() {
        where_
    } else {
        format!("{tip}\n\n{where_}")
    };
    if resp.on_hover_text(tip).clicked() {
        let (old, new) = match r.change {
            Change::Removed => (r.old_line, None),
            _ => (None, r.new_line),
        };
        acts.push(Action::DiffGoToLine { old, new });
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn header_blocks_pointer_without_adding_a_dead_keyboard_stop() {
        let mut h = Harness::new_ui_state(
            |ui, initialized: &mut bool| {
                if !std::mem::replace(initialized, true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                ui.button("Before table").clicked();
                let (rect, _) = ui.allocate_exact_size(vec2(200.0, ROW_H), Sense::hover());
                header(ui, &Tokens::get(ui.ctx()), &[], &TableDiff::default(), rect);
                ui.button("After table").clicked();
            },
            false,
        );
        h.run_steps(2);
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label("Before table").is_focused());
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label("After table").is_focused());
    }
}
