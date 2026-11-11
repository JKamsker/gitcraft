//! The radar's picture: the branches on a circle around the default branch, each pair that
//! overlaps or conflicts a node between its two, shaped by what it is; the legend, the table
//! of every pair, and the details of what is picked.

mod details;
pub(super) use details::{details, table};

use std::collections::HashSet;

use egui::{Pos2, RichText, Ui, pos2};

use super::{Act, Item, Layout, Picked, act};
use crate::insights::Evidence;
use crate::insights::conflict_radar::read::FileHunks;
use crate::insights::conflict_radar::{Edge, Radar};
use crate::insights::constellation::short;
use crate::repo::insights::common::{self, Mark};
use crate::repo::insights::graph::{self, Graph, NodeShape};
use crate::repo::{Action, DiffKey};
use crate::theme::{self, Tokens};

/// How close two nodes of the picture may be (in the unit square).
const ROOM: f32 = 0.07;

/// Where the nodes go: `branches` on a circle from the top, the default branch (`target`) in
/// the middle, and each pair of `pairs` on the line between its two, as near its middle as
/// leaves it clear of the others.
pub(super) fn place(branches: usize, target: bool, pairs: &[(usize, usize)]) -> Vec<Pos2> {
    let mut pos: Vec<Pos2> = (0..branches)
        .map(|i| {
            let a = i as f32 / branches.max(1) as f32 * std::f32::consts::TAU
                - std::f32::consts::FRAC_PI_2;
            pos2(0.5 + 0.44 * a.cos(), 0.5 + 0.44 * a.sin())
        })
        .collect();
    if target {
        // Two branches face each other across the middle: the default branch goes aside, or
        // all three and their pairs would be on one line.
        pos.push(if branches == 2 {
            pos2(0.86, 0.5)
        } else {
            pos2(0.5, 0.5)
        });
    }
    let ends = pos.clone();
    for &(a, b) in pairs {
        let (Some(&pa), Some(&pb)) = (ends.get(a), ends.get(b)) else {
            pos.push(pos2(0.5, 0.5));
            continue;
        };
        let at = |t: f32| pa + (pb - pa) * t;
        let clear = |p: Pos2| pos.iter().all(|q| (*q - p).length() >= ROOM);
        let spot = [0.5, 0.42, 0.58, 0.34, 0.66, 0.26, 0.74]
            .iter()
            .map(|&t| at(t))
            .find(|&p| clear(p))
            .unwrap_or_else(|| at(0.5));
        pos.push(spot);
    }
    pos
}

/// The picture of `radar`, the pairs with a branch in `moved` faded as stale.
pub(super) fn layout(radar: &Radar, moved: &HashSet<usize>, t: &Tokens) -> Layout {
    let mut g = Graph::default();
    let mut items = Vec::new();
    let now = crate::git::now();
    for (i, n) in radar.nodes.iter().enumerate() {
        let target = radar.has_target && i + 1 == radar.nodes.len();
        let mut tip = vec![format!("tip {}", short(&n.tip))];
        if target {
            tip.push("the default branch: what the others land on".to_owned());
        }
        if n.worktree {
            tip.push("checked out in a worktree".to_owned());
        }
        if n.committed > 0 {
            tip.push(format!(
                "last committed {}",
                crate::git::relative_time(n.committed, now)
            ));
        }
        if moved.contains(&i) {
            tip.push("its tip moved since: read again".to_owned());
        }
        g.nodes.push(graph::Node {
            tip: tip.join("\n"),
            shape: if target {
                NodeShape::Square
            } else {
                NodeShape::Circle
            },
            badge: if target {
                Some("T".to_owned())
            } else {
                n.worktree.then(|| "W".to_owned())
            },
            faded: moved.contains(&i),
            ..graph::Node::new(n.name.clone(), t.accent)
        });
        items.push(Item::Branch(i));
    }
    let shown: Vec<usize> = (0..radar.edges.len())
        .filter(|&i| radar.edges[i].shown())
        .collect();
    let mut edge_pairs = Vec::new();
    for &i in &shown {
        let e = &radar.edges[i];
        let stale = moved.contains(&e.a) || moved.contains(&e.b);
        let (shape, color, evidence, badge) = if e.result.merge_error().is_some() {
            (
                NodeShape::Circle,
                t.text_dim,
                Evidence::Unknown,
                "?".to_owned(),
            )
        } else if !e.result.conflicts().is_empty() {
            (
                NodeShape::Diamond,
                t.danger,
                Evidence::Observed,
                e.result.conflicts().len().to_string(),
            )
        } else {
            (
                NodeShape::Square,
                t.warning,
                Evidence::Observed,
                e.result.overlap.len().to_string(),
            )
        };
        let node = g.nodes.len();
        g.nodes.push(graph::Node {
            tip: pair_tip(radar, e, stale),
            shape,
            evidence,
            badge: Some(badge),
            faded: stale,
            size: 0.9,
            ..graph::Node::new(String::new(), color)
        });
        items.push(Item::Pair(i));
        for (from, to) in [(e.a, node), (node, e.b)] {
            g.edges.push(graph::Edge {
                weight: e.weight().max(1) as f32,
                evidence,
                color: Some(color),
                tip: pair_tip(radar, e, stale),
                ..graph::Edge::new(from, to)
            });
            edge_pairs.push(i);
        }
    }
    let pairs: Vec<(usize, usize)> = shown
        .iter()
        .map(|&i| (radar.edges[i].a, radar.edges[i].b))
        .collect();
    Layout {
        positions: place(radar.branches().len(), radar.has_target, &pairs),
        graph: g,
        items,
        edge_pairs,
        moved: moved.clone(),
    }
}

/// What a pair is, in a few lines.
fn pair_tip(radar: &Radar, e: &Edge, stale: bool) -> String {
    let (a, b) = (&radar.nodes[e.a].name, &radar.nodes[e.b].name);
    let mut lines = vec![format!("{a} and {b}"), verdict(e)];
    lines.push(read_word(e, stale).to_owned());
    lines.join("\n")
}

/// What Git said of a pair, in a line.
fn verdict(e: &Edge) -> String {
    let r = &e.result;
    if let Some(err) = r.merge_error() {
        return format!("couldn't be merged in memory: {err}");
    }
    let conflicts = r.conflicts().len();
    let overlap = r.overlap.len();
    match (conflicts, overlap) {
        (0, 0) => "no file changed on both sides; merges cleanly".to_owned(),
        (0, n) => format!("{n} files changed on both since their merge-base; merges cleanly"),
        (c, n) => format!("a dry-run merge leaves {c} files conflicted ({n} changed on both)"),
    }
}

/// Whether a pair was worked out now, taken from the last read, or is stale.
fn read_word(e: &Edge, stale: bool) -> &'static str {
    if stale {
        "stale: a tip moved since"
    } else if e.fresh {
        "worked out by this read"
    } else {
        "reused: its tips didn't move since the last read"
    }
}

/// A line counting the pairs.
pub(super) fn summary(radar: &Radar) -> String {
    let conflicting = radar
        .edges
        .iter()
        .filter(|e| !e.result.conflicts().is_empty())
        .count();
    let overlapping = radar
        .edges
        .iter()
        .filter(|e| e.result.conflicts().is_empty() && !e.result.overlap.is_empty())
        .count();
    let unknown = radar
        .edges
        .iter()
        .filter(|e| e.result.merge_error().is_some())
        .count();
    let mut text = format!(
        "{} pairs: {conflicting} conflict, {overlapping} overlap only",
        radar.edges.len()
    );
    if unknown > 0 {
        text.push_str(&format!(", {unknown} not known"));
    }
    text.push_str(&format!(
        " · {} merged in memory now, {} reused (tips unchanged)",
        radar.computed, radar.reused
    ));
    text
}

pub(super) fn legend(ui: &mut Ui, t: &Tokens) {
    common::legend(
        ui,
        &[
            (
                Mark::Glyph("3", t.danger),
                "conflicts (diamond): files a dry-run merge leaves conflicted",
            ),
            (
                Mark::Glyph("2", t.warning),
                "overlap only (square): files both changed, merging cleanly",
            ),
            (
                Mark::Glyph("?", t.text_dim),
                "not known (dotted): couldn't be merged in memory",
            ),
            (Mark::Glyph("T", t.accent), "the default branch"),
            (Mark::Glyph("W", t.accent), "checked out in a worktree"),
        ],
    );
    ui.label(
        RichText::new(
            "Lines are as thick as the number of files; faded pairs are stale (a tip moved \
             since). A pair with no file in common has no node; the table lists every pair.",
        )
        .size(11.5)
        .color(t.text_faint),
    );
}
