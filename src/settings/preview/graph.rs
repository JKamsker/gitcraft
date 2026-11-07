//! Previews of the commit graph's settings: a small made-up history drawn as the graph draws
//! one, with the column or the lanes the setting changes marked and measured.

use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, Ui, pos2, vec2};

use super::{area, draw};
use crate::git::Commit;
use crate::graph::{self, Graph};
use crate::repo::graph_view::{COMPACT_LANE_W, LANE_W, ROW_GAP, ROW_PITCH, edge};
use crate::settings::Settings;
use crate::theme::{self, LaneColors, ThemeId, Tokens, Tweaks};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RefKind {
    /// The branch checked out.
    Head,
    Local,
    Remote,
    Tag,
}

struct Mock {
    hash: &'static str,
    parents: &'static [&'static str],
    summary: &'static str,
    author: &'static str,
    hash7: &'static str,
    when: &'static str,
    date: &'static str,
    refs: &'static [(&'static str, RefKind)],
}

/// A short history: a feature branch, a merged fix, a release tag.
const HISTORY: [Mock; 7] = [
    Mock {
        hash: "a",
        parents: &["c"],
        summary: "Preview the settings beside them",
        author: "Ada Lovelace",
        hash7: "3f9a2c1",
        when: "5 minutes ago",
        date: "2026-11-06 14:02",
        refs: &[("feature/settings-previews", RefKind::Head)],
    },
    Mock {
        hash: "b",
        parents: &["d", "e"],
        summary: "Merge pull request #212 from fix/tab-overflow",
        author: "Grace Hopper",
        hash7: "b81e07d",
        when: "2 hours ago",
        date: "2026-11-06 12:20",
        refs: &[("main", RefKind::Local), ("origin/main", RefKind::Remote)],
    },
    Mock {
        hash: "c",
        parents: &["d"],
        summary: "Theme gallery with live thumbnails",
        author: "Ada Lovelace",
        hash7: "77c4e90",
        when: "yesterday",
        date: "2026-11-05 18:44",
        refs: &[],
    },
    Mock {
        hash: "e",
        parents: &["d"],
        summary: "Tabs scroll when there are too many",
        author: "Linus Torvalds",
        hash7: "0d5be33",
        when: "2 days ago",
        date: "2026-11-04 09:13",
        refs: &[(
            "origin/fix/tab-strip-overflows-in-small-windows",
            RefKind::Remote,
        )],
    },
    Mock {
        hash: "d",
        parents: &["f"],
        summary: "Release 0.9.0",
        author: "Grace Hopper",
        hash7: "e2a9f41",
        when: "last week",
        date: "2026-10-30 16:00",
        refs: &[("v0.9.0", RefKind::Tag)],
    },
    Mock {
        hash: "f",
        parents: &["g"],
        summary: "Read Base16 colour schemes",
        author: "Linus Torvalds",
        hash7: "5c0d8b2",
        when: "2 weeks ago",
        date: "2026-10-22 11:31",
        refs: &[],
    },
    Mock {
        hash: "g",
        parents: &[],
        summary: "Initial commit",
        author: "Ada Lovelace",
        hash7: "9b7f3aa",
        when: "a month ago",
        date: "2026-10-01 08:00",
        refs: &[],
    },
];

fn history_graph() -> Graph {
    let commits: Vec<Commit> = HISTORY
        .iter()
        .map(|m| Commit {
            hash: m.hash.to_owned(),
            parents: m.parents.iter().map(|p| (*p).to_owned()).collect(),
            ..Commit::default()
        })
        .collect();
    graph::layout(&commits)
}

/// Where each part of a row is: the left edges and widths of the columns drawn.
struct Layout {
    left: f32,
    /// The branch column as drawn, and how wide it really is.
    refs: f32,
    refs_true: f32,
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

/// The header strip and `rows` rows of the history in `rect`, laid out by `l`.
fn paint_graph(p: &Painter, t: &Tokens, rect: Rect, l: &Layout, rows: usize) {
    let g = history_graph();
    p.rect_filled(rect, 0.0, t.dock);
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
    p.rect_filled(header, 0.0, t.chrome);
    p.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let font = theme::semibold(10.0);
    let right = rect.right();
    let desc_x = l.gx() + l.graph + 6.0;
    let mut labels = Vec::new();
    if l.refs > 0.0 {
        labels.push((l.left + 10.0, "BRANCH / TAG"));
    }
    labels.push((l.gx() + 8.0, "GRAPH"));
    labels.push((desc_x, "DESCRIPTION"));
    let mut x = right - l.trailing_w();
    for (name, w, _) in &l.trailing {
        labels.push((x, name));
        x += w;
    }
    for (i, (x, label)) in labels.iter().enumerate() {
        let limit = labels.get(i + 1).map_or(right, |(next, _)| next - 6.0);
        draw::text(
            p,
            pos2(*x, header.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            t.text_faint,
            limit - x,
        );
    }

    for (i, (m, row)) in HISTORY.iter().zip(&g.rows).take(rows).enumerate() {
        let top = header.bottom() + i as f32 * ROW_PITCH;
        let r = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), ROW_PITCH));
        if i == 0 {
            p.rect_filled(r.shrink2(vec2(0.0, ROW_GAP / 2.0)), 0.0, t.row_selected);
        }
        let mid = r.center().y;
        let color = t.lane(row.col);
        let node = pos2(l.lane_x(row.col), mid);

        // Pills, joined to the dot.
        if let Some((first, kind)) = m.refs.first()
            && l.refs > 0.0
        {
            let more = m.refs.len() - 1;
            let more_w = if more > 0 { 28.0 } else { 0.0 };
            let max_w = (l.refs_true - 10.0 - 5.0 - more_w - 14.0).max(40.0);
            let clip = p.with_clip_rect(Rect::from_min_max(
                pos2(l.left, r.top()),
                pos2(l.gx() - 4.0, r.bottom()),
            ));
            let fill = match kind {
                RefKind::Head | RefKind::Local => color,
                RefKind::Remote => color.lerp_to_gamma(Color32::BLACK, 0.35),
                RefKind::Tag => Color32::from_rgb(104, 104, 112),
            };
            let pill = draw::pill(
                &clip,
                t,
                pos2(l.left + 8.0, mid),
                first,
                fill,
                *kind == RefKind::Head,
                max_w,
            );
            let mut end = pill.right();
            if more > 0 {
                let m = Rect::from_min_size(pos2(end + 3.0, mid - 10.0), vec2(25.0, 20.0));
                clip.rect_filled(m, t.radius, color.gamma_multiply(0.25));
                clip.text(
                    m.center(),
                    Align2::CENTER_CENTER,
                    format!("+{more}"),
                    theme::medium(10.5),
                    t.text,
                );
                end = m.right();
            }
            let end = end.min(l.gx() - 4.0);
            p.line_segment(
                [pos2(end, mid), pos2(node.x, mid)],
                Stroke::new(1.5, color.gamma_multiply(0.8)),
            );
        }

        // Lanes and the dot.
        let gp = p.with_clip_rect(Rect::from_min_max(
            pos2(l.gx(), r.top()),
            pos2(l.gx() + l.graph, r.bottom()),
        ));
        for &lane in row.through.iter() {
            gp.line_segment(
                [
                    pos2(l.lane_x(lane), r.top()),
                    pos2(l.lane_x(lane), r.bottom()),
                ],
                Stroke::new(2.0, t.lane(lane)),
            );
        }
        for &lane in row.incoming.iter() {
            edge(&gp, node, pos2(l.lane_x(lane), r.top()), t.lane(lane), 2.0);
        }
        // The last row drawn: its lines run on, as into older history.
        for &lane in row.outgoing.iter() {
            edge(
                &gp,
                node,
                pos2(l.lane_x(lane), r.bottom()),
                t.lane(lane),
                2.0,
            );
        }
        if m.parents.len() > 1 {
            gp.circle_filled(node, 3.5, color);
        } else {
            gp.circle_filled(node, 6.0, color);
            if i == 0 {
                gp.circle_filled(node, 2.5, t.dock);
            }
        }

        // The description, then the trailing columns.
        let trailing_left = right - l.trailing_w();
        draw::text(
            p,
            pos2(desc_x + 2.0, mid),
            Align2::LEFT_CENTER,
            m.summary,
            FontId::proportional(12.5),
            t.text,
            trailing_left - desc_x - 12.0,
        );
        let mut x = trailing_left;
        let small = FontId::proportional(11.5);
        for (_, w, what) in &l.trailing {
            let room = w - 8.0;
            match what {
                Trailing::Hash => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        m.hash7,
                        theme::mono(11.0),
                        t.text_faint,
                        room,
                    );
                }
                Trailing::Author => {
                    let c = pos2(x + 10.0, mid);
                    p.circle_filled(c, 10.0, t.lane(name_hash(m.author)));
                    p.text(
                        c,
                        Align2::CENTER_CENTER,
                        initials(m.author),
                        theme::semibold(8.0),
                        Color32::WHITE,
                    );
                    draw::text(
                        p,
                        pos2(x + 26.0, mid),
                        Align2::LEFT_CENTER,
                        m.author,
                        small.clone(),
                        t.text_dim,
                        room - 26.0,
                    );
                }
                Trailing::When => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        m.when,
                        small.clone(),
                        t.text_dim,
                        room,
                    );
                }
                Trailing::Date => {
                    draw::text(
                        p,
                        pos2(x, mid),
                        Align2::LEFT_CENTER,
                        m.date,
                        theme::mono(11.0),
                        t.text_faint,
                        room,
                    );
                }
            }
            x += w;
        }
    }
}

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
    let refs_true = s.refs_width.max(210.0);
    // Room is kept for the graph and the start of the descriptions.
    let refs = refs_true.min(width - graph - 56.0).max(120.0);
    let l = Layout {
        left: rect.left(),
        refs,
        refs_true,
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
        &format!("{refs_true:.0} px"),
    );
}

/// The columns shown right of the descriptions, at their widths (in proportion, when they
/// don't all fit).
pub(super) fn columns(ui: &mut Ui, t: &Tokens, s: &Settings, width: f32) {
    let rect = area(ui, width, MEASURE_H + HEADER_H + ROWS as f32 * ROW_PITCH);
    let p = ui.painter_at(rect);
    let mut trailing = Vec::new();
    for (shown, name, w, what) in [
        (s.show_hash, "COMMIT", s.hash_width, Trailing::Hash),
        (s.show_author, "AUTHOR", s.author_width, Trailing::Author),
        (s.show_when, "WHEN", s.when_width, Trailing::When),
        (s.show_date, "DATE", s.date_width, Trailing::Date),
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
            "No columns: the descriptions take the whole width",
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
            &format!("{real:.0}"),
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
        ("Standard", LANE_W, !s.compact_graph),
        ("Compact", COMPACT_LANE_W, s.compact_graph),
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
            &format!("{lane_w:.0}"),
        );
        draw::choice(
            &p,
            t,
            body,
            &format!("{name} · {lane_w:.0} px apart"),
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
            name,
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
