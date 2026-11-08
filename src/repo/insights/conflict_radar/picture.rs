//! The radar's picture: the branches on a circle around the default branch, each pair that
//! overlaps or conflicts a node between its two, shaped by what it is; the legend, the table
//! of every pair, and the details of what is picked.

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
        pos.push(pos2(0.5, 0.5));
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

/// Every pair, clickable.
pub(super) fn table(
    ui: &mut Ui,
    radar: &Radar,
    moved: &HashSet<usize>,
    selected: Option<Picked>,
    acts: &mut Vec<Action>,
) {
    let widths = [180.0, 180.0, 90.0, 90.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Branch".into(),
            "With".into(),
            "Conflicts".into(),
            "Overlap".into(),
            "Read".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("conflict-radar-table")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, e) in radar.edges.iter().enumerate() {
                let stale = moved.contains(&e.a) || moved.contains(&e.b);
                let conflicts = match e.result.merge_error() {
                    Some(_) => "not known".to_owned(),
                    None => e.result.conflicts().len().to_string(),
                };
                let resp = common::table_row(
                    ui,
                    &widths,
                    &[
                        radar.nodes[e.a].name.clone(),
                        radar.nodes[e.b].name.clone(),
                        conflicts,
                        e.result.overlap.len().to_string(),
                        read_word(e, stale).to_owned(),
                    ],
                    false,
                    selected == Some(Picked::Pair(i)),
                );
                if resp.clicked() {
                    acts.push(act(Act::SelectPair(i)));
                }
            }
            order(ui, radar, acts);
        });
}

/// The landing order, each branch clickable.
fn order(ui: &mut Ui, radar: &Radar, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let Some(target) = radar.target() else {
        ui.label(
            RichText::new("No default branch found: no landing order to suggest.")
                .size(11.5)
                .color(t.text_faint),
        );
        return;
    };
    common::heading(
        ui,
        &format!("A possible order to land them on {}", target.name),
    );
    for (n, step) in radar.order.iter().enumerate() {
        let name = &radar.nodes[step.branch].name;
        let mut why = Vec::new();
        if step.with_target > 0 {
            why.push(format!(
                "{} files conflict with {}",
                step.with_target, target.name
            ));
        }
        for (earlier, files) in &step.with_earlier {
            why.push(format!(
                "{files} files conflict with {}",
                radar.nodes[*earlier].name
            ));
        }
        if !step.unknown.is_empty() {
            why.push("not known against some landed before".to_owned());
        }
        let text = if step.clean() {
            format!("{}. {name}: lands cleanly", n + 1)
        } else {
            format!("{}. {name}: {}", n + 1, why.join("; "))
        };
        if ui.small_button(text).clicked() {
            acts.push(act(Act::SelectBranch(step.branch)));
        }
    }
    ui.label(
        RichText::new(
            "From the dry-run merges: the most branches that conflict with none of each other \
             first, then the fewest files to resolve.",
        )
        .size(11.5)
        .color(t.text_faint),
    );
}

/// What is picked: a pair's files and regions, or a branch's pairs.
pub(super) fn details(
    ui: &mut Ui,
    radar: &Radar,
    moved: &HashSet<usize>,
    picked: Picked,
    hunks: Option<&Vec<FileHunks>>,
    (loading, error): (bool, Option<&str>),
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    match picked {
        Picked::Branch(i) => {
            let Some(n) = radar.nodes.get(i) else {
                return;
            };
            common::heading(ui, &n.name);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("tip {}", short(&n.tip))).size(12.0));
                if ui.button("Show its tip commit").clicked() {
                    acts.push(Action::SelectHash(n.tip.clone()));
                }
                buttons(ui, acts);
            });
            ui.horizontal_wrapped(|ui| {
                for (k, e) in radar.edges.iter().enumerate() {
                    if (e.a == i || e.b == i) && e.shown() {
                        let other = &radar.nodes[if e.a == i { e.b } else { e.a }].name;
                        let text = format!("with {other}: {}", short_verdict(e));
                        if ui.small_button(text).clicked() {
                            acts.push(act(Act::SelectPair(k)));
                        }
                    }
                }
            });
            order(ui, radar, acts);
        }
        Picked::Pair(k) => {
            let Some(e) = radar.edges.get(k) else {
                return;
            };
            let (a, b) = (&radar.nodes[e.a], &radar.nodes[e.b]);
            common::heading(ui, &format!("{} and {}", a.name, b.name));
            let stale = moved.contains(&e.a) || moved.contains(&e.b);
            ui.label(RichText::new(verdict(e)).size(12.0));
            ui.label(
                RichText::new(format!(
                    "{} @ {} and {} @ {} · {}",
                    a.name,
                    short(&a.tip),
                    b.name,
                    short(&b.tip),
                    read_word(e, stale)
                ))
                .size(11.5)
                .color(if stale { t.danger } else { t.text_faint }),
            );
            if let Some(why) = &e.result.partial {
                ui.label(
                    RichText::new(format!("partial: {why}"))
                        .size(11.5)
                        .color(t.warning),
                );
            }
            ui.horizontal_wrapped(|ui| {
                if let Some(base) = &e.result.base {
                    let counts = |tip: &str| e.result.changed.get(tip).copied().unwrap_or(0);
                    ui.label(
                        RichText::new(format!(
                            "merge-base {} · {} changed {} files, {} changed {}",
                            short(base),
                            a.name,
                            counts(&a.tip),
                            b.name,
                            counts(&b.tip)
                        ))
                        .size(11.5)
                        .color(t.text_dim),
                    );
                    if ui.small_button("Show the merge-base").clicked() {
                        acts.push(Action::SelectHash(base.clone()));
                    }
                } else {
                    ui.label(
                        RichText::new("No merge-base: they share no history")
                            .size(11.5)
                            .color(t.text_dim),
                    );
                }
                buttons(ui, acts);
            });
            let base = e.result.base.as_deref();
            if !e.result.conflicts().is_empty() {
                common::heading(ui, "Conflicted by a dry-run merge");
                for path in e.result.conflicts() {
                    path_row(ui, path, base, (&a.name, &a.tip), (&b.name, &b.tip), acts);
                    regions(ui, path, hunks, loading, error, (&a.name, &b.name));
                }
            }
            let only = e.result.overlap_only();
            if !only.is_empty() {
                common::heading(ui, "Changed on both, merging cleanly");
                for path in only {
                    path_row(ui, path, base, (&a.name, &a.tip), (&b.name, &b.tip), acts);
                }
            }
            order(ui, radar, acts);
        }
    }
}

fn short_verdict(e: &Edge) -> String {
    if e.result.merge_error().is_some() {
        "not known".to_owned()
    } else if e.result.conflicts().is_empty() {
        format!("{} overlapping", e.result.overlap.len())
    } else {
        format!("{} conflicting", e.result.conflicts().len())
    }
}

fn buttons(ui: &mut Ui, acts: &mut Vec<Action>) {
    if ui
        .button("Collision matrix")
        .on_hover_text("Every pair of the worktrees' branches, merged in memory")
        .clicked()
    {
        acts.push(Action::OpenCollisions);
    }
    if ui.button("Deselect (Esc)").clicked() {
        acts.push(act(Act::Deselect));
    }
}

/// A file, with a button for each side's diff of it since the merge-base.
fn path_row(
    ui: &mut Ui,
    path: &str,
    base: Option<&str>,
    a: (&str, &str),
    b: (&str, &str),
    acts: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(path).font(theme::mono(12.0)));
        let Some(base) = base else {
            return;
        };
        for (name, tip) in [a, b] {
            if ui
                .small_button(format!("{name}'s change"))
                .on_hover_text(format!(
                    "{path} from the merge-base {} to {name} @ {}",
                    short(base),
                    short(tip)
                ))
                .clicked()
            {
                acts.push(Action::OpenDiff(DiffKey::Range {
                    from: base.to_owned(),
                    to: Some(tip.to_owned()),
                    path: path.to_owned(),
                }));
            }
        }
    });
}

/// The conflicted regions of `path`, as the dry-run merge wrote them.
fn regions(
    ui: &mut Ui,
    path: &str,
    hunks: Option<&Vec<FileHunks>>,
    loading: bool,
    error: Option<&str>,
    (a, b): (&str, &str),
) {
    let t = Tokens::get(ui.ctx());
    let faint = |s: String| RichText::new(s).size(11.5).color(t.text_faint);
    let Some(file) = hunks.and_then(|h| h.iter().find(|f| f.path == path)) else {
        let said = match (loading, error) {
            (true, _) => "reading the conflicted regions…".to_owned(),
            (_, Some(e)) => format!("regions not read: {e}"),
            _ => "regions not read (past the files read for a pair)".to_owned(),
        };
        ui.label(faint(said));
        return;
    };
    match &file.hunks {
        Err(e) => {
            ui.label(faint(format!("regions not read: {e}")));
        }
        Ok(list) if list.is_empty() => {
            ui.label(faint(
                "no conflicted text (a file deleted or renamed on one side, or binary)".to_owned(),
            ));
        }
        Ok(list) => {
            for h in list {
                let mut text = format!("line {}\n<<<<<<< {a}\n", h.line);
                for l in &h.ours {
                    text.push_str(l);
                    text.push('\n');
                }
                text.push_str("=======\n");
                for l in &h.theirs {
                    text.push_str(l);
                    text.push('\n');
                }
                text.push_str(&format!(">>>>>>> {b}"));
                if h.cut {
                    text.push_str("\n(lines past the first ones left out)");
                }
                egui::Frame::new()
                    .fill(t.field)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).font(theme::mono(11.5)).color(t.text),
                            )
                            .wrap(),
                        );
                    });
                ui.add_space(2.0);
            }
            if file.more {
                ui.label(faint("more regions than shown".to_owned()));
            }
        }
    }
    ui.add_space(2.0);
}
