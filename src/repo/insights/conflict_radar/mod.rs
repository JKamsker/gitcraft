//! Branch conflict radar (#175 item 6): the active branches and worktrees joined by the files
//! they both change and the conflicts a dry-run merge of them finds. Each pair is a node of
//! its own between its two branches, so it can be reached with the keyboard and told by its
//! shape; picked, it lists the exact files (and the conflicted regions), each opening either
//! side's diff since the merge-base, and an order to land the branches in.

mod picture;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use egui::{Pos2, RichText, Ui, vec2};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use super::graph::{self, Graph, Pick};
use crate::insights::conflict_radar::read::{self, FileHunks};
use crate::insights::conflict_radar::{Cache, Key, Radar, key};
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    read: Slot<Radar>,
    /// The pairs worked out so far, by their tips: read again, those whose tips didn't move
    /// are taken from here.
    cache: Cache,
    layout: Option<Layout>,
    selected: Option<Picked>,
    /// The conflicted regions of the pair picked, read when it is picked.
    hunks: Slot<Vec<FileHunks>>,
    hunks_for: Option<Key>,
}

/// What is picked: a pair (by its place among the radar's edges) or a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Picked {
    Pair(usize),
    Branch(usize),
}

/// What a node of the picture stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Branch(usize),
    Pair(usize),
}

/// The picture of the radar, made once it is read and again when a tip moves.
struct Layout {
    graph: Graph,
    positions: Vec<Pos2>,
    /// What each of the graph's nodes stands for.
    items: Vec<Item>,
    /// The pair each of the graph's edges belongs to.
    edge_pairs: Vec<usize>,
    /// The branches whose tips moved since, as it was made with.
    moved: HashSet<usize>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    SelectPair(usize),
    SelectBranch(usize),
    Deselect,
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::ConflictRadar(a))
}

impl RepoTab {
    /// Reads what the view shows (again): only the pairs whose tips moved are merged again.
    pub(super) fn conflict_radar_load(&mut self, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.conflict_radar;
        let request = state.read.start();
        let cache = state.cache.clone();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || read::radar(&repo, cache, crate::git::now()),
            move |tab, result, _| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                let state = &mut view.conflict_radar;
                let (result, cache) = match result {
                    Ok((radar, cache)) => (Ok(radar), Some(cache)),
                    Err(e) => (Err(e), None),
                };
                let was = state
                    .selected
                    .and_then(|p| names_of(state.read.ready()?, p));
                if !state.read.accept(request, result) {
                    return;
                }
                if let Some(cache) = cache {
                    state.cache = cache;
                }
                state.layout = None;
                // What was picked stays picked when its branches are still there.
                state.selected = was.and_then(|w| find(state.read.ready()?, &w));
            },
        );
    }

    pub(super) fn conflict_radar_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.conflict_radar;
        match act {
            Act::SelectPair(i) => {
                state.selected = Some(Picked::Pair(i));
                let Some(radar) = state.read.ready() else {
                    return;
                };
                let Some(edge) = radar.edges.get(i) else {
                    return;
                };
                let (a, b) = (
                    radar.nodes[edge.a].tip.clone(),
                    radar.nodes[edge.b].tip.clone(),
                );
                let conflicts = edge.result.conflicts().to_vec();
                let k = key(&a, &b);
                if conflicts.is_empty() || state.hunks_for.as_ref() == Some(&k) {
                    return;
                }
                state.hunks_for = Some(k);
                let request = state.hunks.start();
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || read::conflict_hunks(&repo, &a, &b, &conflicts),
                    move |tab, found, _| {
                        if let Some(view) = tab.insights.as_mut() {
                            view.conflict_radar.hunks.accept(request, found);
                        }
                    },
                );
            }
            Act::SelectBranch(i) => state.selected = Some(Picked::Branch(i)),
            Act::Deselect => state.selected = None,
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn conflict_radar_back(&mut self) -> bool {
        self.insights
            .as_mut()
            .is_some_and(|v| v.conflict_radar.selected.take().is_some())
    }

    pub(super) fn conflict_radar_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        // A tip moved, as far as the tab knows (one not loaded yet has no branches to tell
        // by): the pairs with it are stale.
        let current: HashMap<String, String> = self
            .refs
            .local
            .iter()
            .map(|b| (b.name.clone(), b.target.clone()))
            .collect();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.conflict_radar;
        match common::slot_state(ui, &state.read, "the branches' overlaps and conflicts") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(radar) = state.read.ready() else {
            return;
        };
        let moved = if current.is_empty() {
            HashSet::new()
        } else {
            // A default branch only on the remote isn't among the tab's branches: not told.
            let remote_target = radar
                .target()
                .filter(|t| !current.contains_key(&t.name))
                .map(|t| (t.name.clone(), t.tip.clone()));
            radar.moved(|name| {
                current.get(name).cloned().or_else(|| {
                    remote_target
                        .as_ref()
                        .filter(|(n, _)| n == name)
                        .map(|(_, tip)| tip.clone())
                })
            })
        };
        let t = Tokens::get(ui.ctx());
        if state.layout.as_ref().is_none_or(|l| l.moved != moved) {
            state.layout = Some(picture::layout(radar, &moved, &t));
        }
        let Some(layout) = state.layout.as_ref() else {
            return;
        };
        let mut provenance = radar.provenance.clone();
        if !moved.is_empty() {
            let mut names: Vec<&str> = moved
                .iter()
                .map(|&i| radar.nodes[i].name.as_str())
                .collect();
            names.sort_unstable();
            provenance.stale = Some(format!("{} moved since", names.join(", ")));
        }
        common::provenance(ui, &provenance);
        ui.label(
            RichText::new(picture::summary(radar))
                .size(11.5)
                .color(t.text_dim),
        );
        picture::legend(ui, &t);
        if radar.branches().is_empty() {
            common::empty(
                ui,
                &format!(
                    "No active branches: none is checked out in a worktree or was committed to \
                     in the last {} days.",
                    crate::insights::conflict_radar::RECENT_DAYS
                ),
            );
            return;
        }
        let selected = state.selected;
        let details_h = if selected.is_some() { 230.0 } else { 0.0 };
        let room = (ui.available_height() - details_h).max(240.0);
        let width = ui.available_width();
        ui.allocate_ui(vec2(width, room), |ui| {
            if table {
                picture::table(ui, radar, &moved, selected, acts);
            } else {
                let shown = selected.and_then(|p| {
                    layout.items.iter().position(|item| match (item, p) {
                        (Item::Pair(i), Picked::Pair(j)) | (Item::Branch(i), Picked::Branch(j)) => {
                            *i == j
                        }
                        _ => false,
                    })
                });
                let picked = graph::show(
                    ui,
                    "conflict-radar",
                    &layout.graph,
                    &layout.positions,
                    shown.map(Pick::Node),
                    room.max(200.0),
                );
                match picked.clicked {
                    Some(Pick::Node(n)) => match layout.items.get(n) {
                        Some(Item::Pair(i)) => acts.push(act(Act::SelectPair(*i))),
                        Some(Item::Branch(i)) => acts.push(act(Act::SelectBranch(*i))),
                        None => {}
                    },
                    Some(Pick::Edge(e)) => {
                        if let Some(&i) = layout.edge_pairs.get(e) {
                            acts.push(act(Act::SelectPair(i)));
                        }
                    }
                    None => {}
                }
            }
        });
        if let Some(p) = selected {
            let hunks = state.hunks.ready().filter(|_| {
                matches!(p, Picked::Pair(i) if radar.edges.get(i).is_some_and(|e| {
                    Some(key(&radar.nodes[e.a].tip, &radar.nodes[e.b].tip)) == state.hunks_for
                }))
            });
            let hunks_state = (state.hunks.loading, state.hunks.error());
            egui::ScrollArea::vertical()
                .id_salt("conflict-radar-details")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    picture::details(ui, radar, &moved, p, hunks, hunks_state, acts);
                });
        }
    }
}

/// The branches of what is picked, by name.
fn names_of(radar: &Radar, p: Picked) -> Option<(String, Option<String>)> {
    match p {
        Picked::Pair(i) => {
            let e = radar.edges.get(i)?;
            Some((
                radar.nodes[e.a].name.clone(),
                Some(radar.nodes[e.b].name.clone()),
            ))
        }
        Picked::Branch(i) => Some((radar.nodes.get(i)?.name.clone(), None)),
    }
}

/// What has the branches `names` in `radar`.
fn find(radar: &Radar, names: &(String, Option<String>)) -> Option<Picked> {
    let at = |name: &str| radar.nodes.iter().position(|n| n.name == name);
    let a = at(&names.0)?;
    match &names.1 {
        Some(b) => {
            let b = at(b)?;
            radar
                .edges
                .iter()
                .position(|e| (e.a, e.b) == (a.min(b), a.max(b)))
                .map(Picked::Pair)
        }
        None => Some(Picked::Branch(a)),
    }
}
