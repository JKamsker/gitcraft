//! The constellation's picture: the graph widget's nodes and edges made of the model, the
//! legend, the table, and the details of what is picked.

use std::collections::HashMap;

use egui::{Color32, RichText, Ui, vec2};

use super::{Act, Built, COLUMNS, act};
use crate::insights::Evidence;
use crate::insights::constellation::read::{self, Disk};
use crate::insights::constellation::{self as model, Flag, Kind, Link, Model};
use crate::repo::Action;
use crate::repo::actions::{BranchRef, compare_branches_action};
use crate::repo::insights::common::{self, Mark};
use crate::repo::insights::graph::{self, Graph, NodeShape};
use crate::repo::versions::VersionsAct;
use crate::theme::Tokens;

/// The colour a node is drawn in: its most telling state, else its kind's.
fn color_of(node: &model::Node, t: &Tokens) -> Color32 {
    let has = |f: Flag| node.flags.iter().any(|(g, _)| *g == f);
    if has(Flag::Failed) {
        t.danger
    } else if has(Flag::Missing) || has(Flag::Merged) {
        t.text_faint
    } else if has(Flag::Dirty) || has(Flag::Running) {
        t.warning
    } else if has(Flag::Active) || has(Flag::Passed) {
        t.success
    } else {
        match node.kind {
            Kind::Branch => t.accent,
            Kind::Worktree => t.text_dim,
            Kind::Task => t.warning,
            Kind::Pr => t.accent,
            Kind::Build => t.text_dim,
        }
    }
}

/// What a worktree's folder takes, said in a line.
fn disk_line(
    node: &model::Node,
    disk: Option<&HashMap<String, Result<Disk, String>>>,
    remote: bool,
) -> Option<String> {
    if node.kind != Kind::Worktree {
        return None;
    }
    if remote {
        return Some("disk: not measured on another machine".to_owned());
    }
    Some(match disk.and_then(|d| d.get(&node.key)) {
        Some(Ok(d)) if d.partial => format!(
            "disk: at least {} in {} files (the walk stopped at {} entries)",
            read::human_bytes(d.bytes),
            d.files,
            read::DISK_ENTRIES
        ),
        Some(Ok(d)) => format!(
            "disk: {} in {} files (its .git aside)",
            read::human_bytes(d.bytes),
            d.files
        ),
        Some(Err(e)) => format!("disk: not measured ({e})"),
        None if disk.is_some() => "disk: not measured (folder missing)".to_owned(),
        None => "disk: being measured…".to_owned(),
    })
}

/// A node's flags as words, each saying how it is known when it is not observed.
fn flag_words(node: &model::Node) -> String {
    node.flags
        .iter()
        .map(|(f, e)| match e {
            Evidence::Observed => f.label().to_owned(),
            _ => format!("{} ({})", f.label(), e.label()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The graph widget's picture of the model.
pub(super) fn to_graph(
    m: &Model,
    disk: Option<&HashMap<String, Result<Disk, String>>>,
    remote: bool,
    t: &Tokens,
) -> Graph {
    // Only the columns with nodes in them take room.
    let present: Vec<Kind> = COLUMNS
        .iter()
        .copied()
        .filter(|k| m.nodes.iter().any(|n| n.kind == *k))
        .collect();
    let nodes = m
        .nodes
        .iter()
        .map(|n| {
            let glyphs: String = n.flags.iter().take(3).map(|(f, _)| f.glyph()).collect();
            let shape = match n.kind {
                Kind::Branch | Kind::Build => NodeShape::Circle,
                Kind::Worktree => NodeShape::Square,
                Kind::Task => NodeShape::Diamond,
                Kind::Pr => NodeShape::Pill,
            };
            let badge = match n.kind {
                Kind::Pr if glyphs.is_empty() => Some("PR".to_owned()),
                Kind::Pr => Some(format!("PR {glyphs}")),
                _ if glyphs.is_empty() => None,
                _ => Some(glyphs.clone()),
            };
            let mut tip = vec![n.kind.label().to_owned()];
            if !n.flags.is_empty() {
                tip.push(flag_words(n));
            }
            tip.extend(n.detail.iter().cloned());
            tip.extend(disk_line(n, disk, remote));
            graph::Node {
                tip: tip.join("\n"),
                shape,
                column: present.iter().position(|k| *k == n.kind).unwrap_or(0),
                badge,
                faded: n
                    .flags
                    .iter()
                    .any(|(f, _)| matches!(f, Flag::Merged | Flag::Missing)),
                size: 1.0 + 0.2 * glyphs.chars().count() as f32,
                ..graph::Node::new(n.label.clone(), color_of(n, t))
            }
        })
        .collect();
    let edges = m
        .edges
        .iter()
        .map(|e| {
            let (a, b) = (&m.nodes[e.from], &m.nodes[e.to]);
            let failed = e.link == Link::Built && b.flags.iter().any(|(f, _)| *f == Flag::Failed);
            graph::Edge {
                evidence: e.evidence,
                directed: e.link == Link::StackParent,
                color: failed.then_some(t.danger),
                weight: if e.link == Link::SameBase { 1.0 } else { 1.5 },
                tip: format!("{} {} {}\n{}", a.label, e.link.label(), b.label, e.note),
                ..graph::Edge::new(e.from, e.to)
            }
        })
        .collect();
    Graph { nodes, edges }
}

/// The legend: kinds by shape, states by letter, links by line.
pub(super) fn legend(ui: &mut Ui, t: &Tokens) {
    ui.label(
        RichText::new(
            "Shapes: circle a branch or a commit's builds, square a worktree, diamond an agent \
             task, PR a pull request",
        )
        .size(11.5)
        .color(t.text_dim),
    );
    let entries: Vec<(Mark, String)> = Flag::ALL
        .iter()
        .map(|f| {
            let color = match f {
                Flag::Failed => t.danger,
                Flag::Dirty | Flag::Running => t.warning,
                Flag::Active | Flag::Passed => t.success,
                _ => t.text_dim,
            };
            let label = match f {
                Flag::Active => {
                    "active (a checkpoint turn in the last 15 min: inferred)".to_owned()
                }
                Flag::Idle => "idle (clean worktree, or no recent turn)".to_owned(),
                Flag::Unpublished => "unpublished (no upstream, or ahead of it)".to_owned(),
                Flag::Merged => "merged into the default branch".to_owned(),
                other => other.label().to_owned(),
            };
            (Mark::Glyph(f.glyph(), color), label)
        })
        .collect();
    let refs: Vec<(Mark, &str)> = entries.iter().map(|(m, l)| (*m, l.as_str())).collect();
    common::legend(ui, &refs);
    common::legend(
        ui,
        &[
            (
                common::evidence_line(Evidence::Observed, t.text_dim),
                "observed (git, the task list, the forge)",
            ),
            (
                common::evidence_line(Evidence::Inferred, t.text_dim),
                "inferred: same fork point from the default branch",
            ),
        ],
    );
}

/// The table: a row per node, clickable.
pub(super) fn nodes_table(
    ui: &mut Ui,
    m: &Model,
    selected: Option<usize>,
    other: Option<usize>,
    acts: &mut Vec<Action>,
) {
    let widths = [100.0, 240.0, 200.0];
    common::table_row(
        ui,
        &widths,
        &[
            "Kind".into(),
            "Name".into(),
            "States".into(),
            "Joined to".into(),
        ],
        true,
        false,
    );
    egui::ScrollArea::vertical()
        .id_salt("constellation-table")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, n) in m.nodes.iter().enumerate() {
                let links: Vec<String> = m
                    .links_of(i)
                    .iter()
                    .map(|(e, j)| format!("{} {}", e.link.label(), m.nodes[*j].label))
                    .collect();
                let resp = common::table_row(
                    ui,
                    &widths,
                    &[
                        n.kind.label().to_owned(),
                        n.label.clone(),
                        flag_words(n),
                        links.join("; "),
                    ],
                    false,
                    selected == Some(i) || other == Some(i),
                );
                if resp.clicked() {
                    let add = ui.input(|inp| inp.modifiers.command);
                    acts.push(act(Act::Select { node: i, add }));
                }
            }
        });
}

/// What the node picked is, what it is joined to, and what opens it.
pub(super) fn details(
    ui: &mut Ui,
    built: &Built,
    i: usize,
    other: Option<usize>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let m = &built.model;
    let Some(n) = m.nodes.get(i) else {
        return;
    };
    common::heading(ui, &format!("{} {}", n.kind.label(), n.label));
    if !n.flags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for (f, e) in &n.flags {
                let (rect, _) = ui.allocate_exact_size(vec2(16.0, 14.0), egui::Sense::hover());
                common::paint_mark(ui, rect, Mark::Glyph(f.glyph(), t.text_dim));
                let word = match e {
                    Evidence::Observed => f.label().to_owned(),
                    _ => format!("{} ({})", f.label(), e.label()),
                };
                ui.label(RichText::new(word).size(12.0));
                ui.add_space(6.0);
            }
        });
    }
    for line in &n.detail {
        ui.add(egui::Label::new(RichText::new(line).size(12.0).color(t.text_dim)).wrap());
    }
    let tip_line = built
        .graph
        .nodes
        .get(i)
        .map(|g| g.tip.as_str())
        .unwrap_or("");
    if let Some(disk) = tip_line.lines().find(|l| l.starts_with("disk:")) {
        ui.label(RichText::new(disk).size(12.0).color(t.text_dim));
    }
    ui.horizontal_wrapped(|ui| {
        match n.kind {
            Kind::Branch => {
                if let Some(tip) = &n.commit
                    && ui.button("Show its tip commit").clicked()
                {
                    acts.push(Action::SelectHash(tip.clone()));
                }
            }
            Kind::Worktree => {
                let missing = n.flags.iter().any(|(f, _)| *f == Flag::Missing);
                if ui
                    .add_enabled(!missing, egui::Button::new("Open worktree"))
                    .clicked()
                {
                    acts.push(Action::OpenWorktree(n.key.clone().into()));
                }
                if ui
                    .add_enabled(!missing, egui::Button::new("Terminal there"))
                    .on_hover_text("A new terminal in this worktree's folder")
                    .clicked()
                {
                    acts.push(Action::NewTerminal {
                        shell: None,
                        cwd: Some(n.key.clone().into()),
                    });
                }
                if let Some(head) = &n.commit
                    && ui.button("Show its HEAD commit").clicked()
                {
                    acts.push(Action::SelectHash(head.clone()));
                }
            }
            Kind::Task => {
                let missing = n.flags.iter().any(|(f, _)| *f == Flag::Missing);
                if ui
                    .add_enabled(!missing, egui::Button::new("Open its worktree"))
                    .clicked()
                {
                    acts.push(Action::OpenWorktree(n.key.clone().into()));
                }
            }
            Kind::Pr => {
                if let Ok(number) = n.key.parse::<u64>()
                    && ui.button(format!("Open pull request #{number}")).clicked()
                {
                    acts.push(Action::OpenPr(number));
                }
            }
            Kind::Build => {
                if ui.button("Open the builds").clicked() {
                    acts.push(Action::OpenCi {
                        sha: n.key.clone(),
                        run: None,
                    });
                }
            }
        }
        if ui.button("Deselect (Esc)").clicked() {
            acts.push(act(Act::Deselect));
        }
    });
    if n.kind == Kind::Branch {
        compare(ui, m, i, other, acts);
    }
    let links = m.links_of(i);
    if !links.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new("Joined to").size(11.5).color(t.text_faint));
        ui.horizontal_wrapped(|ui| {
            for (e, j) in links {
                let text = format!("{} {}", e.link.label(), m.nodes[j].label);
                let text = if e.evidence == Evidence::Observed {
                    text
                } else {
                    format!("{text} ({})", e.evidence.label())
                };
                if ui.small_button(text).on_hover_text(&e.note).clicked() {
                    acts.push(act(Act::Select {
                        node: j,
                        add: false,
                    }));
                }
            }
        });
    }
}

/// Comparing the branch picked with a second one: picked with Ctrl or Cmd and a click, or
/// from the list.
fn compare(ui: &mut Ui, m: &Model, i: usize, other: Option<usize>, acts: &mut Vec<Action>) {
    let a = &m.nodes[i];
    ui.horizontal_wrapped(|ui| {
        let shown = other.map_or("Compare with…".to_owned(), |j| m.nodes[j].label.clone());
        egui::ComboBox::from_id_salt("constellation-compare")
            .selected_text(shown)
            .width(200.0)
            .show_ui(ui, |ui| {
                for (j, n) in m.nodes.iter().enumerate() {
                    if n.kind == Kind::Branch
                        && j != i
                        && ui.selectable_label(other == Some(j), &n.label).clicked()
                    {
                        acts.push(act(Act::CompareWith(j)));
                    }
                }
            });
        let Some(b) = other.and_then(|j| m.nodes.get(j)) else {
            return;
        };
        if ui
            .button("Compare versions")
            .on_hover_text(format!(
                "The commits each of {} and {} has that the other hasn't, paired (range-diff)",
                a.label, b.label
            ))
            .clicked()
        {
            acts.push(Action::Versions(VersionsAct::Branches {
                old: format!("refs/heads/{}", a.key),
                old_label: a.label.clone(),
                new: format!("refs/heads/{}", b.key),
                new_label: b.label.clone(),
            }));
        }
        if ui
            .button("Diff")
            .on_hover_text(format!(
                "What {} changes since it forked from {}",
                b.label, a.label
            ))
            .clicked()
        {
            acts.push(compare_branches_action(
                BranchRef::local(&a.key),
                BranchRef::local(&b.key),
                true,
            ));
        }
        if ui
            .button("Collision matrix")
            .on_hover_text("Merges the worktrees' branches pair by pair, in memory")
            .clicked()
        {
            acts.push(Action::OpenCollisions);
        }
    });
}
