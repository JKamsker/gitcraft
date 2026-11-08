//! A node-link picture for the insights views: nodes in columns, on a circle or placed by a
//! force layout; edges as thick as their weight and solid, dashed or dotted by how they are
//! known. Dragging pans, Ctrl+scroll (or a pinch) zooms and a double click fits it again;
//! nodes take the keyboard (Tab, Enter) like buttons, and hovering one brings out its edges.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use super::common;
use crate::insights::Evidence;
use crate::theme::{self, Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeShape {
    Circle,
    Square,
    Diamond,
    /// A rounded box wide enough for its label inside.
    Pill,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    pub(crate) label: String,
    /// The tooltip.
    pub(crate) tip: String,
    pub(crate) color: Color32,
    pub(crate) shape: NodeShape,
    /// A solid outline when observed, dashed when inferred, dotted when unknown.
    pub(crate) evidence: Evidence,
    /// Its column, for [`Arrange::Columns`].
    pub(crate) column: usize,
    /// A letter or two drawn in it (`!`, `?`, `PR`), so its kind isn't told by colour alone.
    pub(crate) badge: Option<String>,
    /// Drawn faint: there, but not what the view is about.
    pub(crate) faded: bool,
    /// Its size, 1 by default.
    pub(crate) size: f32,
}

impl Node {
    pub(crate) fn new(label: impl Into<String>, color: Color32) -> Self {
        Self {
            label: label.into(),
            tip: String::new(),
            color,
            shape: NodeShape::Circle,
            evidence: Evidence::Observed,
            column: 0,
            badge: None,
            faded: false,
            size: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Edge {
    pub(crate) from: usize,
    pub(crate) to: usize,
    /// Its width, 1 to 6.
    pub(crate) weight: f32,
    pub(crate) evidence: Evidence,
    pub(crate) color: Option<Color32>,
    /// An arrow at `to`.
    pub(crate) directed: bool,
    pub(crate) tip: String,
}

impl Edge {
    pub(crate) fn new(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            weight: 1.0,
            evidence: Evidence::Observed,
            color: None,
            directed: false,
            tip: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Graph {
    pub(crate) nodes: Vec<Node>,
    pub(crate) edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrange {
    /// By each node's column, left to right, in the order they come in each column.
    Columns,
    /// Connected nodes pulled together, others pushed apart (worked out once; give it a
    /// background thread for a large graph).
    Force,
}

/// Where each node goes, in a unit square (0 to 1 both ways).
pub(crate) fn arrange(graph: &Graph, how: Arrange) -> Vec<Pos2> {
    match how {
        Arrange::Columns => {
            let columns = graph
                .nodes
                .iter()
                .map(|x| x.column)
                .max()
                .map_or(0, |m| m + 1);
            let mut per_column = vec![0usize; columns];
            for node in &graph.nodes {
                per_column[node.column] += 1;
            }
            let mut seen = vec![0usize; columns];
            graph
                .nodes
                .iter()
                .map(|node| {
                    let c = node.column;
                    let i = seen[c];
                    seen[c] += 1;
                    pos2(
                        (c as f32 + 0.5) / columns as f32,
                        (i as f32 + 0.5) / per_column[c] as f32,
                    )
                })
                .collect()
        }
        Arrange::Force => force(graph),
    }
}

fn circle(n: usize) -> Vec<Pos2> {
    if n == 1 {
        return vec![pos2(0.5, 0.5)];
    }
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            pos2(0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin())
        })
        .collect()
}

/// Fruchterman and Reingold's layout, from the circle, the same every time for a graph.
fn force(graph: &Graph) -> Vec<Pos2> {
    let n = graph.nodes.len();
    let mut pos: Vec<Vec2> = circle(n).into_iter().map(|p| p.to_vec2()).collect();
    if n < 3 {
        return pos.into_iter().map(|v| v.to_pos2()).collect();
    }
    let k = (1.0 / n as f32).sqrt();
    let mut temperature = 0.1;
    let iterations = if n > 300 { 60 } else { 200 };
    for _ in 0..iterations {
        let mut moves = vec![Vec2::ZERO; n];
        for i in 0..n {
            for j in (i + 1)..n {
                let d = pos[i] - pos[j];
                let dist = d.length().max(0.001);
                let push = d / dist * (k * k / dist);
                moves[i] += push;
                moves[j] -= push;
            }
        }
        for e in &graph.edges {
            if e.from == e.to || e.from >= n || e.to >= n {
                continue;
            }
            let d = pos[e.from] - pos[e.to];
            let dist = d.length().max(0.001);
            let pull = d / dist * (dist * dist / k) * e.weight.clamp(0.5, 3.0);
            moves[e.from] -= pull;
            moves[e.to] += pull;
        }
        for i in 0..n {
            let len = moves[i].length();
            if len > 0.0 {
                pos[i] += moves[i] / len * len.min(temperature);
            }
            // Drawn towards the middle, so that parts not joined don't drift off.
            let pull = (vec2(0.5, 0.5) - pos[i]) * 0.01;
            pos[i] += pull;
        }
        temperature *= 0.97;
    }
    // Into the unit square.
    let (mut min, mut max) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &pos {
        min = min.min(*p);
        max = max.max(*p);
    }
    let span = (max - min).max(Vec2::splat(0.001));
    pos.into_iter()
        .map(|p| {
            let v = (p - min) / span;
            pos2(0.04 + v.x * 0.92, 0.04 + v.y * 0.92)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    Node(usize),
    Edge(usize),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Picked {
    pub(crate) clicked: Option<Pick>,
    pub(crate) hovered: Option<Pick>,
}

/// How far the picture is zoomed and panned, kept between frames.
#[derive(Clone, Copy, Debug)]
struct View {
    zoom: f32,
    pan: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

const NODE_R: f32 = 9.0;

/// Draws `graph` with its nodes at `positions` (from [`arrange`]) in the room left, at least
/// `min_height` tall. `selected`: what is marked as picked.
pub(crate) fn show(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    graph: &Graph,
    positions: &[Pos2],
    selected: Option<Pick>,
    min_height: f32,
) -> Picked {
    let t = Tokens::get(ui.ctx());
    let id = Id::new(("insight-graph", salt));
    // In a scrolled area the height left may be unbounded: then `min_height` it is.
    let left = ui.available_height();
    let height = if left.is_finite() && left < 20_000.0 {
        left.max(min_height)
    } else {
        min_height
    };
    let size = vec2(ui.available_width(), height);
    let (rect, canvas) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, t.radius, t.dock);
    let mut view: View = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if canvas.dragged() {
        view.pan += canvas.drag_delta();
    }
    if canvas.double_clicked() {
        view = View::default();
    }
    if canvas.hovered() {
        let zoom = ui.input(|i| i.zoom_delta());
        if zoom != 1.0 {
            let pointer = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let before = (pointer - rect.center() - view.pan) / view.zoom;
            view.zoom = (view.zoom * zoom).clamp(0.3, 6.0);
            view.pan = pointer - rect.center() - before * view.zoom;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, view));
    let inner = rect.shrink(36.0);
    let to_screen = |p: Pos2| -> Pos2 {
        let base = inner.min + vec2(p.x * inner.width(), p.y * inner.height());
        rect.center() + (base - rect.center()) * view.zoom + view.pan
    };
    let screen: Vec<Pos2> = (0..graph.nodes.len())
        .map(|i| to_screen(positions.get(i).copied().unwrap_or(pos2(0.5, 0.5))))
        .collect();
    let radius = |n: &Node| NODE_R * n.size.clamp(0.5, 3.0) * view.zoom.sqrt();
    let mut picked = Picked::default();

    // Nodes take the pointer and the keyboard first; then edges near the pointer.
    let mut node_resps = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let r = radius(node);
        let hit = node_rect(node, screen[i], r, ui);
        if !rect.intersects(hit) {
            node_resps.push(None);
            continue;
        }
        let resp = ui.interact(hit.intersect(rect), id.with(("node", i)), Sense::click());
        crate::a11y::a11y_selected(
            &resp,
            || format!("{}: {}", node.label, node.tip),
            selected == Some(Pick::Node(i)),
        );
        if resp.hovered() || resp.has_focus() {
            picked.hovered = Some(Pick::Node(i));
        }
        if resp.clicked() {
            picked.clicked = Some(Pick::Node(i));
        }
        node_resps.push(Some(resp));
    }
    if picked.hovered.is_none()
        && let Some(pointer) = canvas.hover_pos()
    {
        let mut best: Option<(usize, f32)> = None;
        for (i, e) in graph.edges.iter().enumerate() {
            let (Some(&a), Some(&b)) = (screen.get(e.from), screen.get(e.to)) else {
                continue;
            };
            let d = distance_to_segment(pointer, a, b);
            if d < 6.0 + e.weight && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        if let Some((i, _)) = best {
            picked.hovered = Some(Pick::Edge(i));
            if canvas.clicked() {
                picked.clicked = Some(Pick::Edge(i));
            }
        }
    }
    let focus = match picked.hovered.or(selected) {
        Some(Pick::Node(i)) => Some(i),
        _ => None,
    };

    // Edges under the nodes.
    for (i, e) in graph.edges.iter().enumerate() {
        let (Some(&a), Some(&b)) = (screen.get(e.from), screen.get(e.to)) else {
            continue;
        };
        let lit = picked.hovered == Some(Pick::Edge(i))
            || selected == Some(Pick::Edge(i))
            || focus.is_some_and(|f| f == e.from || f == e.to);
        let base = e.color.unwrap_or(t.text_faint);
        let color = if lit {
            if e.color.is_some() { base } else { t.text_dim }
        } else if focus.is_some() {
            base.gamma_multiply(0.25)
        } else {
            base.gamma_multiply(0.7)
        };
        let width = e.weight.clamp(1.0, 6.0) * if lit { 1.3 } else { 1.0 };
        let (a, b) = trim(
            a,
            b,
            radius(&graph.nodes[e.from]),
            radius(&graph.nodes[e.to]),
        );
        common::evidence_segment(&painter, &[a, b], e.evidence, Stroke::new(width, color));
        if e.directed {
            arrow(&painter, a, b, color, width);
        }
    }

    // The nodes.
    for (i, node) in graph.nodes.iter().enumerate() {
        let Some(resp) = &node_resps[i] else {
            continue;
        };
        let c = screen[i];
        let r = radius(node);
        let dim = node.faded || focus.is_some_and(|f| f != i && !joined(graph, f, i));
        let color = if dim {
            node.color.gamma_multiply(0.35)
        } else {
            node.color
        };
        let fill = common::tint(color, if dim { 0.15 } else { 0.35 }, &t);
        paint_node(ui, &painter, node, c, r, fill, color);
        if selected == Some(Pick::Node(i)) {
            painter.circle_stroke(c, r + 5.0, Stroke::new(2.0, t.accent));
        }
        if resp.has_focus() {
            painter.circle_stroke(c, r + 7.0, Stroke::new(1.5, t.accent_fg));
        }
        if node.shape != NodeShape::Pill && view.zoom > 0.6 {
            let label_color = if dim { t.text_faint } else { t.text_dim };
            painter.text(
                c + vec2(0.0, r + 3.0),
                Align2::CENTER_TOP,
                elide(&node.label, 28),
                egui::FontId::proportional(11.0),
                label_color,
            );
        }
        if !node.tip.is_empty() {
            resp.clone().on_hover_ui(|ui| {
                ui.label(egui::RichText::new(&node.label).font(theme::medium(12.0)));
                ui.label(format!(
                    "{}\n({} — {})",
                    node.tip,
                    node.evidence.label(),
                    match node.evidence {
                        Evidence::Observed => "solid outline",
                        Evidence::Inferred => "dashed outline",
                        Evidence::Unknown => "dotted outline",
                    }
                ));
            });
        }
    }
    if let Some(Pick::Edge(i)) = picked.hovered
        && let Some(e) = graph.edges.get(i)
        && !e.tip.is_empty()
    {
        canvas.clone().on_hover_text_at_pointer(format!(
            "{}\n({}, {} line)",
            e.tip,
            e.evidence.label(),
            match e.evidence {
                Evidence::Observed => "solid",
                Evidence::Inferred => "dashed",
                Evidence::Unknown => "dotted",
            }
        ));
    }
    if view.zoom != 1.0 || view.pan != Vec2::ZERO {
        painter.text(
            rect.right_bottom() - vec2(8.0, 6.0),
            Align2::RIGHT_BOTTOM,
            format!("{:.0}% · double-click to fit", view.zoom * 100.0),
            egui::FontId::proportional(10.5),
            t.text_faint,
        );
    }
    picked
}

/// Whether nodes `a` and `b` share an edge.
fn joined(graph: &Graph, a: usize, b: usize) -> bool {
    graph
        .edges
        .iter()
        .any(|e| (e.from == a && e.to == b) || (e.from == b && e.to == a))
}

fn node_rect(node: &Node, c: Pos2, r: f32, ui: &Ui) -> Rect {
    match node.shape {
        NodeShape::Pill => {
            let w = pill_width(node, ui);
            Rect::from_center_size(c, vec2(w, r * 2.2))
        }
        _ => Rect::from_center_size(c, Vec2::splat(r * 2.0 + 4.0)),
    }
}

fn pill_width(node: &Node, ui: &Ui) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        elide(&node.label, 28),
        egui::FontId::proportional(11.5),
        Color32::WHITE,
    );
    galley.size().x + 20.0 + if node.badge.is_some() { 18.0 } else { 0.0 }
}

fn paint_node(
    ui: &Ui,
    painter: &egui::Painter,
    node: &Node,
    c: Pos2,
    r: f32,
    fill: Color32,
    color: Color32,
) {
    let t = Tokens::get(ui.ctx());
    let outline: Vec<Pos2> = match node.shape {
        NodeShape::Circle => (0..32)
            .map(|k| {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect(),
        NodeShape::Square => {
            let s = r * 0.9;
            vec![
                c + vec2(-s, -s),
                c + vec2(s, -s),
                c + vec2(s, s),
                c + vec2(-s, s),
            ]
        }
        NodeShape::Diamond => vec![
            c + vec2(0.0, -r * 1.15),
            c + vec2(r * 1.15, 0.0),
            c + vec2(0.0, r * 1.15),
            c + vec2(-r * 1.15, 0.0),
        ],
        NodeShape::Pill => {
            let rect = node_rect(node, c, r, ui);
            painter.rect_filled(rect, rect.height() / 2.0, fill);
            let mut ring = Vec::new();
            let rr = rect.height() / 2.0;
            for k in 0..=16 {
                let a = std::f32::consts::FRAC_PI_2 + k as f32 / 16.0 * std::f32::consts::PI;
                ring.push(pos2(rect.left() + rr, rect.center().y) + vec2(a.cos(), a.sin()) * rr);
            }
            for k in 0..=16 {
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 / 16.0 * std::f32::consts::PI;
                ring.push(pos2(rect.right() - rr, rect.center().y) + vec2(a.cos(), a.sin()) * rr);
            }
            ring.push(ring[0]);
            common::evidence_segment(painter, &ring, node.evidence, Stroke::new(1.5, color));
            let mut x = rect.left() + 10.0;
            if let Some(badge) = &node.badge {
                painter.text(
                    pos2(x, c.y),
                    Align2::LEFT_CENTER,
                    badge,
                    theme::semibold(10.0),
                    color,
                );
                x += 18.0;
            }
            painter.text(
                pos2(x, c.y),
                Align2::LEFT_CENTER,
                elide(&node.label, 28),
                egui::FontId::proportional(11.5),
                t.text,
            );
            return;
        }
    };
    painter.add(Shape::convex_polygon(outline.clone(), fill, Stroke::NONE));
    let mut closed = outline;
    closed.push(closed[0]);
    common::evidence_segment(painter, &closed, node.evidence, Stroke::new(1.5, color));
    if let Some(badge) = &node.badge {
        painter.text(
            c,
            Align2::CENTER_CENTER,
            badge,
            theme::semibold(r.clamp(8.0, 12.0)),
            color,
        );
    }
}

/// The segment from `a` to `b` without the parts inside the nodes.
fn trim(a: Pos2, b: Pos2, ra: f32, rb: f32) -> (Pos2, Pos2) {
    let d = b - a;
    let len = d.length();
    if len <= ra + rb + 1.0 {
        return (a, b);
    }
    let u = d / len;
    (a + u * (ra + 2.0), b - u * (rb + 3.0))
}

fn arrow(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32, width: f32) {
    let d = b - a;
    let len = d.length();
    if len < 1.0 {
        return;
    }
    let u = d / len;
    let n = vec2(-u.y, u.x);
    let size = 5.0 + width;
    painter.add(Shape::convex_polygon(
        vec![
            b,
            b - u * size + n * size * 0.5,
            b - u * size - n * size * 0.5,
        ],
        color,
        Stroke::NONE,
    ));
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 == 0.0 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// `text` cut to `max` characters, with an ellipsis.
pub(crate) fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph {
            nodes: (0..n)
                .map(|i| Node {
                    column: i % 2,
                    ..Node::new(format!("n{i}"), Color32::WHITE)
                })
                .collect(),
            edges: edges.iter().map(|&(a, b)| Edge::new(a, b)).collect(),
        }
    }

    #[test]
    fn every_layout_stays_in_the_unit_square() {
        let g = graph(12, &[(0, 1), (1, 2), (2, 3), (5, 9), (9, 11)]);
        for how in [Arrange::Columns, Arrange::Force] {
            let pos = arrange(&g, how);
            assert_eq!(pos.len(), 12);
            for p in pos {
                assert!(
                    (0.0..=1.0).contains(&p.x) && (0.0..=1.0).contains(&p.y),
                    "{how:?}"
                );
            }
        }
        assert_eq!(
            arrange(&graph(1, &[]), Arrange::Force),
            vec![pos2(0.5, 0.5)]
        );
        assert!(arrange(&graph(0, &[]), Arrange::Columns).is_empty());
    }

    #[test]
    fn the_force_layout_is_the_same_every_time_and_pulls_joined_nodes_together() {
        let g = graph(8, &[(0, 4)]);
        let a = arrange(&g, Arrange::Force);
        assert_eq!(a, arrange(&g, Arrange::Force));
        let joined = (a[0] - a[4]).length();
        let apart = (a[1] - a[5]).length();
        assert!(joined < apart, "{joined} < {apart}");
    }

    #[test]
    fn columns_go_left_to_right() {
        let pos = arrange(&graph(4, &[]), Arrange::Columns);
        assert!(pos[0].x < pos[1].x);
        assert!(pos[0].y < pos[2].y);
    }

    #[test]
    fn labels_are_cut_with_an_ellipsis() {
        assert_eq!(elide("abc", 5), "abc");
        assert_eq!(elide("abcdef", 4), "abc…");
    }
}
