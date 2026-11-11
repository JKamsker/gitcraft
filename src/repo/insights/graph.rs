//! A node-link picture for the insights views: nodes in columns, on a circle or placed by a
//! force layout; edges as thick as their weight and solid, dashed or dotted by how they are
//! known. Dragging pans, Ctrl+scroll (or a pinch) zooms and a double click fits it again;
//! nodes take the keyboard (Tab, Enter) like buttons, and hovering one brings out its edges.

mod painting;
mod scene;
use painting::{arrow, distance_to_segment, node_rect, paint_node, trim};

mod layout;
pub(crate) use layout::arrange;

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

    let scene = scene::Scene {
        graph,
        screen: &screen,
        selected,
        hovered: picked.hovered,
        focus,
        zoom: view.zoom,
        tokens: &t,
    };
    scene.paint_edges(&painter);
    scene.paint_nodes(ui, &painter, &node_resps);
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
