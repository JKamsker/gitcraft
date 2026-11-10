//! Build the coupling view model and its graph from sampled history.

use super::*;

/// Reads `limit` commits of HEAD and counts them.
pub(super) fn read(
    repo: &std::path::Path,
    limit: usize,
    options: &Options,
) -> Result<Model, String> {
    let history = history::read(repo, "HEAD", limit)?;
    Ok(build(Arc::new(history), options))
}

/// Counts `history` with `options`, and lays out the network.
pub(super) fn build(history: Arc<History>, options: &Options) -> Model {
    let coupling = coupling::analyze(&history, options);
    let mut nodes: Vec<usize> = coupling.pairs[..coupling.shown]
        .iter()
        .flat_map(|p| [p.a, p.b])
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    let plain = network(&coupling, &nodes, &Tokens::default(), &[], &[]);
    let positions = graph::arrange(&plain, Arrange::Force);
    let mut provenance = history.provenance();
    provenance.source = "git log --numstat".to_owned();
    let mut partial: Vec<String> = provenance.partial.take().into_iter().collect();
    partial.extend(coupling.cut());
    if !partial.is_empty() {
        provenance.partial = Some(partial.join("; "));
    }
    Model {
        id: common::next_request(),
        history,
        coupling,
        nodes,
        positions,
        provenance,
    }
}

/// The network: `nodes` (units) joined by the pairs shown. `changed` and `left` mark the units
/// a change touches and those it leaves out.
pub(super) fn network(
    c: &Coupling,
    nodes: &[usize],
    t: &Tokens,
    changed: &[usize],
    left: &[usize],
) -> Graph {
    let most = nodes
        .iter()
        .map(|&u| c.units[u].changes)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let strongest = c.pairs.first().map_or(1, |p| p.support).max(1) as f32;
    let marking = !changed.is_empty();
    let nodes_out = nodes
        .iter()
        .map(|&u| {
            let unit = &c.units[u];
            let (color, shape, badge) = if changed.contains(&u) {
                (t.success, NodeShape::Square, Some("C".to_owned()))
            } else if left.contains(&u) {
                (t.warning, NodeShape::Diamond, Some("!".to_owned()))
            } else {
                (t.accent, NodeShape::Circle, None)
            };
            Node {
                tip: format!("changed in {} of the commits counted", unit.changes),
                shape,
                badge,
                faded: marking && shape == NodeShape::Circle,
                size: 0.7 + 1.5 * (unit.changes as f32 / most).sqrt(),
                ..Node::new(label(&unit.name), color)
            }
        })
        .collect();
    let index = |u: usize| nodes.binary_search(&u).unwrap_or(0);
    let edges = c.pairs[..c.shown]
        .iter()
        .map(|p| Edge {
            weight: 1.0 + 5.0 * p.support as f32 / strongest,
            evidence: Evidence::Inferred,
            tip: pair_tip(c, p),
            ..Edge::new(index(p.a), index(p.b))
        })
        .collect();
    Graph {
        nodes: nodes_out,
        edges,
    }
}

/// A unit's name as a node shows it: its file name, or the folder.
pub(super) fn label(name: &str) -> String {
    match name.rsplit_once('/') {
        Some((_, file)) if !file.is_empty() => file.to_owned(),
        _ => name.to_owned(),
    }
}

fn pair_tip(c: &Coupling, p: &coupling::Pair) -> String {
    let (a, b) = (&c.units[p.a].name, &c.units[p.b].name);
    format!(
        "{a} ↔ {b}\nchanged together in {} commits\n{} of {a}'s changes, {} of {b}'s\n\
         a co-change, not a proven dependency",
        p.support,
        percent(c.confidence(p, p.a)),
        percent(c.confidence(p, p.b)),
    )
}

pub(super) fn percent(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}
