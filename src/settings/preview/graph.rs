//! Previews of the commit graph's settings: a small made-up history drawn as the graph draws
//! one, with the column or the lanes the setting changes marked and measured.

use egui::{Align2, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};

use super::{area, draw};
use crate::repo::graph_view::{COMPACT_LANE_W, LANE_W, ROW_PITCH};
use crate::settings::Settings;
use crate::theme::{self, LaneColors, ThemeId, Tokens, Tweaks};

mod history;
use history::history_graph;
mod paint;
use paint::paint_graph;

/// Where each part of a row is: the left edges and widths of the columns drawn.
struct Layout {
    left: f32,
    /// The branch column as drawn, and how wide it really is.
    refs: f32,
    refs_true: f32,
    keep_branch_end: bool,
    graph: f32,
    lane_w: f32,
    /// Trailing columns, left to right: (header, width as drawn).
    trailing: Vec<(&'static str, f32, Trailing)>,
}

#[derive(Clone, Copy)]
enum Trailing {
    Hash,
    Author,
    When,
    Date,
}

impl Layout {
    fn gx(&self) -> f32 {
        self.left + self.refs
    }

    fn lane_x(&self, lane: usize) -> f32 {
        self.gx() + 6.0 + lane as f32 * self.lane_w + self.lane_w / 2.0
    }

    fn trailing_w(&self) -> f32 {
        self.trailing.iter().map(|(_, w, _)| w).sum()
    }
}

const HEADER_H: f32 = 24.0;

pub(super) fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect()
}

pub(super) fn name_hash(name: &str) -> usize {
    name.bytes().map(usize::from).sum()
}

/// Lanes of the history, at `lane_w` apart.
fn graph_width(lane_w: f32) -> f32 {
    history_graph().width as f32 * lane_w + 12.0
}

/// Accent tint over `x0..x1` of `rect`: the part the setting changes.
fn mark(p: &Painter, t: &Tokens, rect: Rect, x0: f32, x1: f32) {
    let band = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
    p.rect_filled(band, 0.0, t.accent.gamma_multiply(0.10));
    let s = Stroke::new(1.0, t.accent.gamma_multiply(0.6));
    p.line_segment([band.left_top(), band.left_bottom()], s);
    p.line_segment([band.right_top(), band.right_bottom()], s);
}

const ROWS: usize = 6;
/// Room above a graph for its measurements.
const MEASURE_H: f32 = 26.0;

/// The branch column at its width (drawn narrower, with a break, when wider than the preview).
pub(super) fn refs(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let rect = area(ui, width, MEASURE_H + HEADER_H + ROWS as f32 * ROW_PITCH);
    let p = ui.painter_at(rect);
    let lane_w = if s.compact_graph {
        COMPACT_LANE_W
    } else {
        LANE_W
    };
    let graph = graph_width(lane_w);
    let refs_true = s.refs_width.max(crate::repo::columns::MIN_REFS_W);
    // Room is kept for the graph and the start of the descriptions.
    let refs = refs_true
        .min(width - graph - 56.0)
        .max(crate::repo::columns::MIN_REFS_W);
    let l = Layout {
        left: rect.left(),
        refs,
        refs_true,
        keep_branch_end: s.branch_names_keep_end,
        graph,
        lane_w,
        trailing: Vec::new(),
    };
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + MEASURE_H), rect.max);
    paint_graph(&p, t, body, &l, ROWS);
    mark(&p, t, body, l.left, l.gx());
    if refs < refs_true {
        draw::zigzag(
            &p,
            t,
            l.left + refs * 0.62,
            body.top(),
            body.bottom(),
            t.dock,
        );
    }
    draw::measure(
        &p,
        t,
        l.left + 1.0,
        l.gx() - 1.0,
        rect.top() + MEASURE_H / 2.0,
        &crate::i18n::message(
            "interaction-preview-refs-width",
            &[("width", refs_true.round().into())],
        ),
    );
}

/// The columns shown right of the descriptions, at their widths (in proportion, when they
/// don't all fit).
pub(super) fn columns(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let rect = area(ui, width, MEASURE_H + HEADER_H + ROWS as f32 * ROW_PITCH);
    let p = ui.painter_at(rect);
    let mut trailing = Vec::new();
    for (shown, name, w, what) in [
        (
            s.show_hash,
            crate::i18n::text("interaction-preview-commit-header"),
            s.hash_width,
            Trailing::Hash,
        ),
        (
            s.show_author,
            crate::i18n::text("interaction-preview-author"),
            s.author_width,
            Trailing::Author,
        ),
        (
            s.show_when,
            crate::i18n::text("interaction-preview-when"),
            s.when_width,
            Trailing::When,
        ),
        (
            s.show_date,
            crate::i18n::text("interaction-preview-date"),
            s.date_width,
            Trailing::Date,
        ),
    ] {
        if shown {
            trailing.push((name, w.max(50.0), what));
        }
    }
    let lane_w = COMPACT_LANE_W;
    let graph = graph_width(lane_w);
    let room = width - graph - 120.0;
    let total: f32 = trailing.iter().map(|(_, w, _)| w).sum();
    let scale = if total > room { room / total } else { 1.0 };
    let real: Vec<f32> = trailing.iter().map(|(_, w, _)| *w).collect();
    for c in &mut trailing {
        c.1 *= scale;
    }
    let l = Layout {
        left: rect.left(),
        refs: 0.0,
        refs_true: 0.0,
        keep_branch_end: s.branch_names_keep_end,
        graph,
        lane_w,
        trailing,
    };
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + MEASURE_H), rect.max);
    paint_graph(&p, t, body, &l, ROWS);
    if l.trailing.is_empty() {
        p.text(
            pos2(rect.center().x, rect.top() + MEASURE_H / 2.0),
            Align2::CENTER_CENTER,
            crate::i18n::text("interaction-preview-no-columns"),
            FontId::proportional(11.5),
            t.text_faint,
        );
        return;
    }
    let mut x = body.right() - l.trailing_w();
    mark(&p, t, body, x, body.right());
    for ((_, w, _), real) in l.trailing.iter().zip(real) {
        draw::measure(
            &p,
            t,
            x + 2.0,
            x + w - 2.0,
            rect.top() + MEASURE_H / 2.0,
            &crate::i18n::number(real.round()),
        );
        x += w;
    }
}

/// The graph's lanes standard and compact, side by side; the one set outlined.
pub(super) fn lanes(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let gap = 14.0;
    let each = (width - gap) / 2.0;
    let rows = 5;
    let h = MEASURE_H + HEADER_H + rows as f32 * ROW_PITCH;
    let rect = area(ui, width, h + 24.0);
    let p = ui.painter_at(rect);
    for (i, (name, lane_w, chosen)) in [
        (
            crate::i18n::text("interaction-preview-standard"),
            LANE_W,
            !s.compact_graph,
        ),
        (
            crate::i18n::text("interaction-preview-compact"),
            COMPACT_LANE_W,
            s.compact_graph,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let r = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (each + gap), rect.top()),
            vec2(each, h),
        );
        let body = Rect::from_min_max(pos2(r.left(), r.top() + MEASURE_H), r.max);
        let l = Layout {
            left: r.left(),
            refs: 0.0,
            refs_true: 0.0,
            keep_branch_end: s.branch_names_keep_end,
            graph: graph_width(lane_w),
            lane_w,
            trailing: Vec::new(),
        };
        let clip = p.with_clip_rect(r);
        paint_graph(&clip, t, body, &l, rows);
        mark(&clip, t, body, l.gx(), l.gx() + l.graph);
        draw::measure(
            &clip,
            t,
            l.lane_x(0),
            l.lane_x(1),
            r.top() + MEASURE_H / 2.0,
            &crate::i18n::number(lane_w.round()),
        );
        draw::choice(
            &p,
            t,
            body,
            &crate::i18n::message(
                "interaction-preview-lane-distance",
                &[("name", name.into()), ("width", lane_w.round().into())],
            ),
            chosen,
        );
    }
}

/// The lanes in the colours set, and the ten colours of each choice.
pub(super) fn lane_colors(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32, current: &ThemeId) {
    // Eight lanes side by side, each with a branch's dot.
    let h = 120.0;
    let rect = area(ui, width, h);
    let p = ui.painter_at(rect);
    p.rect_filled(rect, t.radius, t.dock);
    let n = 8;
    let lane_w = LANE_W + 6.0;
    let gx = rect.left() + 16.0;
    for lane in 0..n {
        let x = gx + lane as f32 * lane_w;
        let color = t.lane(lane);
        let y = rect.top() + 14.0 + lane as f32 * 11.0;
        p.line_segment(
            [pos2(x, y), pos2(x, rect.bottom() - 8.0)],
            Stroke::new(2.0, color),
        );
        p.circle_filled(pos2(x, y), 6.0, color);
    }
    // And pills of their colours beside them.
    let px = gx + n as f32 * lane_w + 10.0;
    for (i, name) in ["main", "feature/themes", "fix/tabs", "release/1.0"]
        .iter()
        .enumerate()
    {
        let y = rect.top() + 20.0 + i as f32 * 26.0;
        draw::pill(
            &p,
            t,
            pos2(px, y),
            draw::PillLabel {
                text: name,
                keep_end: s.branch_names_keep_end,
            },
            t.lane(i),
            i == 0,
            rect.right() - px - 8.0,
        );
    }
    ui.add_space(14.0);
    let info = theme::resolve(current);
    for option in LaneColors::ALL {
        let tweaks = Tweaks {
            lanes: option,
            ..s.theme_tweaks
        };
        let tokens = info.tokens(&tweaks);
        let rect = area(ui, width, 30.0);
        let p = ui.painter_at(rect);
        let chosen = s.theme_tweaks.lanes == option;
        if chosen {
            p.rect_filled(rect, t.radius, t.hover);
            draw::check(&p, pos2(rect.left() + 10.0, rect.center().y), t.accent);
        }
        draw::text(
            &p,
            pos2(rect.left() + 22.0, rect.center().y),
            Align2::LEFT_CENTER,
            option.label(),
            if chosen {
                theme::medium(12.0)
            } else {
                FontId::proportional(12.0)
            },
            if chosen { t.text } else { t.text_dim },
            130.0,
        );
        let sw = ((rect.right() - rect.left() - 160.0) / 10.0).clamp(8.0, 18.0);
        for (i, c) in tokens.lanes.iter().enumerate() {
            let x = rect.right() - 8.0 - (10 - i) as f32 * sw;
            let r = Rect::from_min_size(pos2(x, rect.center().y - 7.0), vec2(sw - 3.0, 14.0));
            p.rect_filled(r, 3.0, *c);
        }
        ui.add_space(2.0);
    }
}

/// A civil-date display for the inert example; its literal time and source record stay unchanged.
fn mock_date(raw: &str) -> String {
    let Some((date, time)) = raw.split_once(' ') else {
        return raw.to_owned();
    };
    crate::i18n::message(
        "interaction-preview-mock-date",
        &[
            ("date", crate::i18n::date_iso(date).into()),
            ("time", time.into()),
        ],
    )
}
