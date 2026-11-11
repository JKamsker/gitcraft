//! The timeline drawn: a row per file or folder with its size, churn and complexity as
//! sparklines over the samples, the test lane's verdicts above them; or the same as tables.
//! A point clicked (or picked with the arrows and Enter in a focused row) selects its window.

mod primitives;
use primitives::{bars, line, point_text, sample_text, verdict_mark};

use egui::{Align2, Color32, Key, Painter, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::{Built, day};
use crate::git::Verdict;
use crate::insights::churn::{Point, Series, Tested, analyzer};
use crate::repo::insights::common::{self, Mark};
use crate::repo::short;
use crate::theme::{self, Tokens};

/// Width of the names on the left.
const NAME_W: f32 = 240.0;
/// Height of a row.
const ROW_H: f32 = 40.0;
/// Height of the header (titles and the tests' strip).
const HEAD_H: f32 = 36.0;
const GAP: f32 = 10.0;

/// The three charts' places in a band starting at `left` (the names' right edge).
fn charts(left: f32, top: f32, h: f32, chart_w: f32) -> [Rect; 3] {
    [0.0, 1.0, 2.0]
        .map(|i| Rect::from_min_size(pos2(left + i * (chart_w + GAP), top), vec2(chart_w, h)))
}

/// Where each of `n` samples is across `rect`.
fn xs(rect: Rect, n: usize) -> Vec<f32> {
    let pad = 6.0;
    if n <= 1 {
        return vec![rect.center().x; n];
    }
    let step = (rect.width() - 2.0 * pad) / (n - 1) as f32;
    (0..n)
        .map(|k| rect.left() + pad + k as f32 * step)
        .collect()
}

/// The sample nearest `x` across `rect`.
fn nearest(rect: Rect, n: usize, x: f32) -> Option<usize> {
    xs(rect, n)
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|(k, _)| k)
}

/// The rows of sparklines under their header; the (row, sample) clicked or picked.
pub(super) fn rows(
    ui: &mut Ui,
    built: &Built,
    selected: Option<(usize, usize)>,
    cursor: &mut usize,
) -> Option<(usize, usize)> {
    let t = Tokens::get(ui.ctx());
    let tl = &built.timeline;
    let n = tl.samples.len();
    let width = ui.available_width();
    let chart_w = ((width - NAME_W - 2.0 * GAP - 12.0) / 3.0).max(80.0);

    // The header: what each chart is, and the tests' verdicts at the samples above each.
    let (head, resp) = ui.allocate_exact_size(vec2(width, HEAD_H), Sense::hover());
    let painter = ui.painter_at(head);
    let titles = [
        "Size, lines · factual",
        "Churn per window · factual",
        "Complexity · heuristic",
    ];
    let places = charts(head.left() + NAME_W, head.top(), HEAD_H, chart_w);
    painter.text(
        pos2(head.left() + 8.0, head.top() + 2.0),
        Align2::LEFT_TOP,
        "File or folder",
        theme::medium(11.5),
        t.text_dim,
    );
    painter.text(
        pos2(head.left() + 8.0, head.bottom() - 2.0),
        Align2::LEFT_BOTTOM,
        "tests at each sample",
        egui::FontId::proportional(10.5),
        t.text_faint,
    );
    // Not one result: said once rather than as a row of question marks, and why.
    let no_results = tl.samples.iter().all(|s| s.verdict.is_none());
    let none_said = if tl.tests_error.is_some() {
        "test results unknown (couldn't be read)"
    } else {
        "no test results recorded"
    };
    for (place, title) in places.iter().zip(titles) {
        // Clipped to its chart: a narrow one doesn't run into the next.
        painter.with_clip_rect(place.shrink2(vec2(1.0, 0.0))).text(
            place.left_top() + vec2(2.0, 2.0),
            Align2::LEFT_TOP,
            title,
            theme::medium(11.5),
            t.text_dim,
        );
        let strip = Rect::from_min_max(pos2(place.left(), place.bottom() - 12.0), place.max);
        if no_results {
            painter.with_clip_rect(*place).text(
                strip.left_center() + vec2(2.0, 0.0),
                Align2::LEFT_CENTER,
                none_said,
                egui::FontId::proportional(10.0),
                t.text_faint,
            );
            continue;
        }
        let step = xs(strip, n).windows(2).next().map_or(10.0, |w| w[1] - w[0]);
        let side = (step - 1.0).clamp(4.0, 11.0);
        for (k, x) in xs(strip, n).into_iter().enumerate() {
            let rect = Rect::from_center_size(pos2(x, strip.center().y), vec2(side, side));
            common::paint_mark(ui, rect, verdict_mark(tl.samples[k].verdict, &t));
        }
    }
    if let Some(pos) = resp.hover_pos()
        && let Some(place) = places.iter().find(|p| p.x_range().contains(pos.x))
        && let Some(k) = nearest(*place, n, pos.x)
    {
        resp.on_hover_text_at_pointer(sample_text(&tl.samples[k]));
    }

    let mut picked = None;
    egui::ScrollArea::vertical()
        .id_salt("insights-churn-rows")
        .auto_shrink([false, false])
        .show_rows(ui, ROW_H, tl.rows.len(), |ui, range| {
            for r in range {
                if let Some(p) = row(ui, built, r, chart_w, selected, cursor) {
                    picked = Some(p);
                }
            }
        });
    picked
}

/// One row: its name and its three sparklines. The sample clicked, or picked with Enter.
fn row(
    ui: &mut Ui,
    built: &Built,
    r: usize,
    chart_w: f32,
    selected: Option<(usize, usize)>,
    cursor: &mut usize,
) -> Option<(usize, usize)> {
    let t = Tokens::get(ui.ctx());
    let tl = &built.timeline;
    let series = tl.rows.get(r)?;
    let n = series.points.len();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let first_last = |v: Option<u64>, w: Option<u64>| {
        let s = |x: Option<u64>| x.map_or("none".to_owned(), |x| x.to_string());
        format!("{} to {}", s(v), s(w))
    };
    let first = series.points.first();
    let last = series.points.last();
    crate::a11y::a11y_selected(
        &resp,
        format!(
            "{}{}: size {} lines, churn {} lines, complexity {}; arrows pick a sample, Enter \
             lists its commits",
            series.shown(),
            if series.hotspot { ", hotspot" } else { "" },
            first_last(first.and_then(|p| p.size), last.and_then(|p| p.size)),
            series.total,
            first_last(
                first.and_then(|p| p.complexity),
                last.and_then(|p| p.complexity)
            ),
        ),
        selected.is_some_and(|s| s.0 == r),
    );
    let painter = ui.painter_at(rect);
    if selected.is_some_and(|s| s.0 == r) {
        painter.rect_filled(rect, 2.0, t.row_selected);
    } else if resp.hovered() {
        painter.rect_filled(rect, 2.0, t.hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect.shrink(1.0),
            2.0,
            Stroke::new(1.5, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let mut name_x = rect.left() + 8.0;
    if series.hotspot {
        let mark = Rect::from_min_size(pos2(name_x, rect.center().y - 6.0), vec2(14.0, 12.0));
        common::paint_mark(ui, mark, Mark::Glyph("!", t.warning));
        name_x += 18.0;
    }
    let name = if series.folder {
        format!("{}/", series.shown())
    } else {
        series.shown().to_owned()
    };
    let galley = painter.layout_no_wrap(name, egui::FontId::proportional(12.0), t.text);
    let name_clip = Rect::from_min_max(
        pos2(name_x, rect.top()),
        pos2(rect.left() + NAME_W - 8.0, rect.bottom()),
    );
    painter.with_clip_rect(name_clip).galley(
        pos2(name_x, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );

    let places = charts(rect.left() + NAME_W, rect.top() + 4.0, ROW_H - 8.0, chart_w);
    for place in &places {
        painter.rect_filled(*place, 2.0, t.field.gamma_multiply(0.5));
    }
    let sizes: Vec<(Option<u64>, bool)> =
        series.points.iter().map(|p| (p.size, p.complete)).collect();
    line(&painter, places[0], &sizes, t.accent, &t);
    let churn: Vec<u64> = series.points.iter().map(|p| p.churn).collect();
    bars(&painter, places[1], &churn, t.accent, &t);
    if series.points.iter().all(|p| p.complexity.is_none()) {
        painter.text(
            places[2].center(),
            Align2::CENTER_CENTER,
            "not measured",
            egui::FontId::proportional(11.0),
            t.text_faint,
        );
    } else {
        let cx: Vec<(Option<u64>, bool)> = series
            .points
            .iter()
            .map(|p| (p.complexity, p.complete))
            .collect();
        line(&painter, places[2], &cx, t.warning, &t);
    }

    let guide = |k: usize, stroke: Stroke| {
        for place in &places {
            if let Some(&x) = xs(*place, n).get(k) {
                painter.line_segment([pos2(x, place.top()), pos2(x, place.bottom())], stroke);
            }
        }
    };
    if let Some((_, k)) = selected.filter(|s| s.0 == r) {
        guide(k, Stroke::new(1.5, t.accent));
    }
    let hovered = resp.hover_pos().and_then(|pos| {
        let place = places.iter().find(|p| p.x_range().contains(pos.x))?;
        nearest(*place, n, pos.x)
    });
    if let Some(k) = hovered {
        guide(k, Stroke::new(1.0, t.text_dim));
    }
    let mut picked = None;
    if resp.has_focus() && n > 0 {
        let (left, right, enter) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::Enter),
            )
        });
        *cursor = (*cursor).min(n - 1);
        if left {
            *cursor = cursor.saturating_sub(1);
        }
        if right {
            *cursor = (*cursor + 1).min(n - 1);
        }
        guide(*cursor, Stroke::new(1.0, t.accent_fg));
        if enter {
            picked = Some((r, *cursor));
        }
        let text = point_text(built, series, *cursor);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &text));
    }
    if resp.clicked() {
        resp.request_focus();
        picked = Some((r, hovered.unwrap_or(n.saturating_sub(1))));
    }
    if let Some(k) = hovered {
        resp.on_hover_text_at_pointer(point_text(built, series, k));
    } else {
        resp.on_hover_text_at_pointer(format!(
            "{}\n{} lines changed over the samples{}",
            series.shown(),
            series.total,
            if series.hotspot {
                format!("\n{}", crate::insights::churn::HOTSPOT_RULE)
            } else {
                String::new()
            }
        ));
    }
    picked
}

/// The tables: every row's first and last sample, and the samples of the row selected; the
/// (row, sample) clicked.
pub(super) fn table(
    ui: &mut Ui,
    built: &Built,
    selected: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let tl = &built.timeline;
    let mut picked = None;
    let bound = |p: Option<&Point>, f: fn(&Point) -> Option<u64>| -> String {
        match p {
            Some(p) => match f(p) {
                Some(v) if p.complete => v.to_string(),
                Some(v) => format!("≥{v}"),
                None => "—".to_owned(),
            },
            None => "—".to_owned(),
        }
    };
    egui::ScrollArea::vertical()
        .id_salt("insights-churn-table")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let widths = [300.0, 130.0, 110.0, 170.0];
            common::table_row(
                ui,
                &widths,
                &[
                    "File or folder".to_owned(),
                    "Size, first → last".to_owned(),
                    "Churn, all windows".to_owned(),
                    "Complexity (heuristic)".to_owned(),
                    "Hotspot".to_owned(),
                ],
                true,
                false,
            );
            for (r, series) in tl.rows.iter().enumerate() {
                let (first, last) = (series.points.first(), series.points.last());
                let measured = series.points.iter().any(|p| p.complexity.is_some());
                let cells = [
                    if series.folder {
                        format!("{}/", series.shown())
                    } else {
                        series.shown().to_owned()
                    },
                    format!("{} → {}", bound(first, |p| p.size), bound(last, |p| p.size)),
                    series.total.to_string(),
                    if measured {
                        format!(
                            "{} → {}",
                            bound(first, |p| p.complexity),
                            bound(last, |p| p.complexity)
                        )
                    } else {
                        "not measured".to_owned()
                    },
                    if series.hotspot { "! hotspot" } else { "" }.to_owned(),
                ];
                let on = selected.is_some_and(|s| s.0 == r);
                if common::table_row(ui, &widths, &cells, false, on).clicked() {
                    picked = Some((r, series.points.len().saturating_sub(1)));
                }
            }
            let Some((r, k_sel)) = selected else {
                return;
            };
            let Some(series) = tl.rows.get(r) else {
                return;
            };
            common::heading(ui, &format!("Samples of {}", series.shown()));
            let widths = [90.0, 90.0, 110.0, 90.0, 130.0, 160.0];
            common::table_row(
                ui,
                &widths,
                &[
                    "Sample".to_owned(),
                    "Date".to_owned(),
                    "Size (lines)".to_owned(),
                    "Churn".to_owned(),
                    "Complexity".to_owned(),
                    "Tests".to_owned(),
                    "Subject".to_owned(),
                ],
                true,
                false,
            );
            for (k, (p, s)) in series.points.iter().zip(&tl.samples).enumerate() {
                let cells = [
                    short(&s.sample.hash).to_owned(),
                    day(s.sample.time),
                    bound(Some(p), |p| p.size),
                    p.churn.to_string(),
                    if p.complexity.is_some() {
                        bound(Some(p), |p| p.complexity)
                    } else {
                        "not measured".to_owned()
                    },
                    match s.verdict {
                        Some(Verdict::Good) => "passed",
                        Some(Verdict::Bad) => "failed",
                        None => "unknown",
                    }
                    .to_owned(),
                    s.sample.subject.clone(),
                ];
                if common::table_row(ui, &widths, &cells, false, k == k_sel)
                    .on_hover_text(point_text(built, series, k))
                    .clicked()
                {
                    picked = Some((r, k));
                }
            }
        });
    picked
}
