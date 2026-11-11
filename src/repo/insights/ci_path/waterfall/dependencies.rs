//! Paint observed/inferred dependency links after waterfall rows establish centers.
use super::*;

pub(super) fn paint(
    painter: &egui::Painter,
    explorer: &Explorer,
    selected: Option<usize>,
    centers: &HashMap<usize, f32>,
    x: &impl Fn(i64) -> f32,
    t: &Tokens,
) {
    let jobs = &explorer.run.jobs;
    let now = explorer.now;
    let mut links: Vec<(usize, usize, Evidence, Color32)> = explorer
        .critical
        .jobs
        .windows(2)
        .filter_map(|w| {
            let dep = explorer
                .deps
                .edges
                .iter()
                .find(|d| d.from == w[0] && d.to == w[1])?;
            Some((w[0], w[1], dep.evidence, t.text))
        })
        .collect();
    if let Some(sel) = selected {
        for d in explorer.deps.before(sel) {
            if !explorer.critical.link(d.from, sel) {
                links.push((d.from, sel, d.evidence, t.text_dim));
            }
        }
    }
    for (from, to, evidence, color) in links {
        let (Some(&ya), Some(&yb)) = (centers.get(&from), centers.get(&to)) else {
            continue;
        };
        let (Some(end), Some(start)) = (
            end_of(&jobs[from], now),
            jobs[to].ready().or(jobs[to].started),
        ) else {
            continue;
        };
        let a = pos2(x(end), ya);
        let b = pos2(x(start).max(a.x + 4.0), yb);
        let knee = a.x + 4.0;
        let points = [a, pos2(knee, ya), pos2(knee, yb), b];
        common::evidence_segment(painter, &points, evidence, Stroke::new(1.5, color));
        painter.add(Shape::convex_polygon(
            vec![b, b + vec2(-5.0, -3.5), b + vec2(-5.0, 3.5)],
            color,
            Stroke::NONE,
        ));
    }
}
