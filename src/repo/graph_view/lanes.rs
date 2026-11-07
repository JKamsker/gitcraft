//! The graph column of a row: the lanes crossing it, the lines to its dot, and the dot.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Ui, pos2, vec2};

use super::CORNER_R;
use super::row::Row;
use crate::icons;
use crate::repo::ghost::{Kind, RowGhost};
use crate::repo::{RepoTab, WIP_HASH};
use crate::theme::Tokens;

impl RepoTab {
    /// The lanes crossing the row, the lines joining its dot to other lanes, and the dot: a
    /// dashed circle for the changes row, a dashed box for a stash.
    pub(super) fn graph_cell(&self, ui: &Ui, painter: &Painter, row: &Row, lanes: usize) {
        let Some(graph_row) = self.graph.rows.get(row.i) else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let is_wip = row.commit.hash == WIP_HASH;
        let stash = self.stash_at(row.i).and_then(|s| self.stashes.get(s));
        let rect = row.rect;
        let (top, bottom) = (rect.top(), rect.bottom());
        let gp = painter.with_clip_rect(rect.intersect(Rect::from_min_size(
            pos2(row.gx, top),
            vec2(row.cols.graph, rect.height()),
        )));
        let x = |lane: usize| row.lane_x(lane);
        let node = pos2(x(graph_row.col), row.mid());
        let width = 2.0;
        // In a ghost run: the lanes to and from the commits it would make, dashed.
        let ghost = self.ghost.row(row.i);
        let new = ghost.is_some_and(|g| g.kind == Kind::New);
        let ghost_lane = |lanes: fn(&RowGhost) -> &Vec<usize>, lane: usize| {
            new || ghost.is_some_and(|g| lanes(g).contains(&lane))
        };

        // The lanes of commits it would leave on no ref go with them: faded.
        let going = |lanes: fn(&RowGhost) -> &Vec<usize>, lane: usize| {
            ghost.is_some_and(|g| lanes(g).contains(&lane))
        };
        for &lane in graph_row.through.iter() {
            if lane < lanes {
                let ends = [pos2(x(lane), top), pos2(x(lane), bottom)];
                if ghost_lane(|g| &g.through, lane) {
                    dashed(&gp, &ends, t.lane(lane), width);
                } else if going(|g| &g.going_through, lane) {
                    gp.line_segment(ends, Stroke::new(width, t.lane(lane).gamma_multiply(GOING)));
                } else {
                    gp.line_segment(ends, Stroke::new(width, t.lane(lane)));
                }
            }
        }
        for &lane in graph_row.incoming.iter() {
            let end = pos2(x(lane), top);
            if ghost_lane(|g| &g.incoming, lane) {
                dashed(&gp, &edge_points(node, end), t.lane(lane), width);
            } else if going(|g| &g.going_incoming, lane) {
                edge(&gp, node, end, t.lane(lane).gamma_multiply(GOING), width);
            } else {
                edge(&gp, node, end, t.lane(lane), width);
            }
        }
        for &lane in graph_row.outgoing.iter() {
            let color = if is_wip || stash.is_some() || self.parked_at(row.i).is_some() {
                t.text_faint
            } else {
                t.lane(lane)
            };
            let end = pos2(x(lane), bottom);
            if new {
                dashed(&gp, &edge_points(node, end), color, width);
            } else {
                edge(&gp, node, end, color, width);
            }
        }

        if stash.is_some() {
            stash_box(ui, &gp, &t, node, row.gx + row.cols.graph);
        } else if let Some(parked) = self.parked_at(row.i) {
            self.parked_marker(ui, &gp, parked, node);
        } else if is_wip {
            gp.circle_filled(node, 6.0, t.dock);
            gp.add(egui::Shape::dashed_line(
                &circle_points(node, 6.0, 16),
                Stroke::new(1.5, t.text_dim),
                2.5,
                2.0,
            ));
        } else if row.commit.parents.len() > 1 {
            gp.circle_filled(node, 3.5, row.color);
        } else {
            gp.circle_filled(node, 6.0, row.color);
            if row.commit.hash == self.refs.head {
                gp.circle_filled(node, 2.5, t.dock);
            }
        }
        match ghost.map(|g| g.kind) {
            // A commit it would make: a dashed ring around its dot.
            Some(Kind::New) => {
                gp.add(egui::Shape::dashed_line(
                    &circle_points(node, 9.0, 24),
                    Stroke::new(1.2, t.text),
                    2.5,
                    2.0,
                ));
            }
            // One it would leave on no ref: crossed out.
            Some(Kind::Going) => {
                // In full, over the faded row, on a halo of the background.
                let mut gp = gp.clone();
                gp.set_opacity(1.0);
                let d = 6.0;
                let arms = [
                    [node + vec2(-d, -d), node + vec2(d, d)],
                    [node + vec2(-d, d), node + vec2(d, -d)],
                ];
                for arm in arms {
                    gp.line_segment(arm, Stroke::new(4.5, t.dock));
                }
                for arm in arms {
                    gp.line_segment(arm, Stroke::new(2.0, t.danger));
                }
            }
            _ => {}
        }
    }
}

/// How much of its colour a lane keeps that leads only to commits a ghost run would leave on
/// no ref.
const GOING: f32 = 0.3;

/// A ghost lane along `points`: dashed and translucent.
fn dashed(p: &Painter, points: &[Pos2], color: Color32, width: f32) {
    p.add(egui::Shape::dashed_line(
        points,
        Stroke::new(width, color.gamma_multiply(0.7)),
        4.0,
        3.0,
    ));
}

/// A stash's dot: a dashed box with the stash icon, as the toolbar's Stash button shows it.
/// `graph_right` is where the graph column ends.
fn stash_box(ui: &Ui, gp: &Painter, t: &Tokens, node: Pos2, graph_right: f32) {
    let r = Rect::from_center_size(node, vec2(15.0, 15.0));
    gp.rect_filled(r, CornerRadius::same(3), t.dock);
    let (l, tp, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    gp.add(egui::Shape::dashed_line(
        &[
            pos2(l, tp),
            pos2(rt, tp),
            pos2(rt, b),
            pos2(l, b),
            pos2(l, tp),
        ],
        Stroke::new(1.5, t.text_dim),
        2.5,
        2.0,
    ));
    // Images ignore the clip; a squeezed graph column leaves the icon out.
    if r.right() <= graph_right {
        icons::paint(ui, r, "archive", 10.0, t.text_dim);
    }
}

fn circle_points(center: Pos2, radius: f32, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            center + vec2(a.cos(), a.sin()) * radius
        })
        .collect()
}

/// Joins a commit's dot to another lane at `end` (the row's top or bottom): straight across
/// the row from the dot, a small rounded corner, then straight along the lane.
pub(crate) fn edge(p: &Painter, node: Pos2, end: Pos2, color: Color32, width: f32) {
    let stroke = Stroke::new(width, color);
    let points = edge_points(node, end);
    if points.len() == 2 {
        p.line_segment([node, end], stroke);
    } else {
        p.add(egui::Shape::line(points, stroke));
    }
}

/// The points of [`edge`]'s line.
fn edge_points(node: Pos2, end: Pos2) -> Vec<Pos2> {
    let dx = end.x - node.x;
    if dx.abs() < 0.5 {
        return vec![node, end];
    }
    let dy = end.y - node.y;
    let r = CORNER_R.min(dx.abs()).min(dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    // The corner's centre: inside the turn, `r` before the lane and `r` past the dot's row.
    let c = pos2(end.x - sx * r, node.y + sy * r);
    let mut points = vec![node];
    const STEPS: usize = 8;
    for k in 0..=STEPS {
        let a = k as f32 / STEPS as f32 * std::f32::consts::FRAC_PI_2;
        points.push(c + vec2(sx * r * a.sin(), -sy * r * a.cos()));
    }
    points.push(end);
    points
}
