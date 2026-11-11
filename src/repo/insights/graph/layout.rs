//! Deterministic graph layout in unit-square coordinates.

use super::*;

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

pub(super) fn circle(n: usize) -> Vec<Pos2> {
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
pub(super) fn force(graph: &Graph) -> Vec<Pos2> {
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
