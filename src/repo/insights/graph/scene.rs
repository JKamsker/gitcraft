//! Read-only graph scene painting, separate from pointer/keyboard interaction.
use super::*;

pub(super) struct Scene<'a> {
    pub graph: &'a Graph,
    pub screen: &'a [Pos2],
    pub selected: Option<Pick>,
    pub hovered: Option<Pick>,
    pub focus: Option<usize>,
    pub zoom: f32,
    pub tokens: &'a Tokens,
}

impl Scene<'_> {
    fn radius(&self, node: &Node) -> f32 {
        NODE_R * node.size.clamp(0.5, 3.0) * self.zoom.sqrt()
    }

    pub fn paint_edges(&self, painter: &egui::Painter) {
        let (graph, screen, t) = (self.graph, self.screen, self.tokens);
        let (selected, focus) = (self.selected, self.focus);
        // Edges under the nodes.
        for (i, e) in graph.edges.iter().enumerate() {
            let (Some(&a), Some(&b)) = (screen.get(e.from), screen.get(e.to)) else {
                continue;
            };
            let lit = self.hovered == Some(Pick::Edge(i))
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
                self.radius(&graph.nodes[e.from]),
                self.radius(&graph.nodes[e.to]),
            );
            common::evidence_segment(painter, &[a, b], e.evidence, Stroke::new(width, color));
            if e.directed {
                arrow(painter, a, b, color, width);
            }
        }
    }

    pub fn paint_nodes(
        &self,
        ui: &mut Ui,
        painter: &egui::Painter,
        node_resps: &[Option<egui::Response>],
    ) {
        let (graph, screen, t) = (self.graph, self.screen, self.tokens);
        let (selected, focus) = (self.selected, self.focus);
        // The nodes.
        for (i, node) in graph.nodes.iter().enumerate() {
            let Some(resp) = &node_resps[i] else {
                continue;
            };
            let c = screen[i];
            let r = self.radius(node);
            let dim = node.faded || focus.is_some_and(|f| f != i && !joined(graph, f, i));
            let color = if dim {
                node.color.gamma_multiply(0.35)
            } else {
                node.color
            };
            let fill = common::tint(color, if dim { 0.15 } else { 0.35 }, t);
            paint_node(ui, painter, node, c, r, fill, color);
            if selected == Some(Pick::Node(i)) {
                painter.circle_stroke(c, r + 5.0, Stroke::new(2.0, t.accent));
            }
            if resp.has_focus() {
                painter.circle_stroke(c, r + 7.0, Stroke::new(1.5, t.accent_fg));
            }
            if node.shape != NodeShape::Pill && self.zoom > 0.6 {
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
    }
}
