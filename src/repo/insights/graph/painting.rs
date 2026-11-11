//! Graph node geometry, shape painting, and edge hit-testing primitives.

use super::*;

pub(super) fn node_rect(node: &Node, c: Pos2, r: f32, ui: &Ui) -> Rect {
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

pub(super) fn paint_node(
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
pub(super) fn trim(a: Pos2, b: Pos2, ra: f32, rb: f32) -> (Pos2, Pos2) {
    let d = b - a;
    let len = d.length();
    if len <= ra + rb + 1.0 {
        return (a, b);
    }
    let u = d / len;
    (a + u * (ra + 2.0), b - u * (rb + 3.0))
}

pub(super) fn arrow(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32, width: f32) {
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

pub(super) fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 == 0.0 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}
