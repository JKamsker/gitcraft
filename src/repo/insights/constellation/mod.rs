//! Worktree and agent constellation (#175 item 3): branches, worktrees, agent tasks, pull
//! requests and builds as one navigable graph, in columns, with a table beside it. A node
//! picked says what it is and opens its worktree, pull request, builds or commit; two branches
//! picked (Ctrl or Cmd with the click) compare, or open the collision matrix.

mod picture;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use egui::{Pos2, RichText, Ui, vec2};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use super::graph::{self, Arrange, Graph, Pick};
use crate::insights::Provenance;
use crate::insights::constellation::read::{self, Disk};
use crate::insights::constellation::{self as model, Forge, Kind, Model, Snapshot};
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;
use picture::{details, legend, nodes_table, to_graph};

/// The columns, left to right.
const COLUMNS: [Kind; 5] = [
    Kind::Branch,
    Kind::Worktree,
    Kind::Task,
    Kind::Pr,
    Kind::Build,
];

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    read: Slot<Snapshot>,
    built: Option<Built>,
    /// Worktree folder → what it takes on disk, read after the rest.
    disk: Slot<HashMap<String, Result<Disk, String>>>,
    selected: Option<usize>,
    /// A second branch picked, to compare with the first.
    other: Option<usize>,
}

/// The constellation drawn from what was read and what the forge said.
struct Built {
    model: Model,
    graph: Graph,
    positions: Vec<Pos2>,
    provenance: Provenance,
    /// Where the forge's pull requests and builds came from, and how freshly.
    forge: String,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Pick a node; `add`: as the second branch, to compare with the first.
    Select {
        node: usize,
        add: bool,
    },
    /// Pick a branch to compare the one picked with.
    CompareWith(usize),
    Deselect,
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Constellation(a))
}

impl RepoTab {
    /// Reads what the view shows (again).
    pub(super) fn constellation_load(&mut self, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let request = view.constellation.read.start();
        let repo = self.path.clone();
        self.insight_then(
            ctx,
            move || read::snapshot(&repo, crate::git::now()),
            move |tab, result, ctx| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.constellation.read.accept(request, result) {
                    tab.constellation_disk(ctx);
                    tab.constellation_build();
                }
            },
        );
    }

    /// What the tab last read of the forge, as the constellation takes it, and where it came
    /// from.
    fn constellation_forge(&self) -> (Forge, String) {
        let prs = self.prs_loaded.map(|_| {
            self.prs
                .iter()
                .map(|p| model::PrIn {
                    number: p.number,
                    title: p.title.clone(),
                    branch: p.branch.clone(),
                    base: p.base.clone(),
                    sha: p.sha.clone(),
                    draft: p.draft,
                    from_fork: p.from_fork,
                })
                .collect()
        });
        let builds = self.ci_project().is_some().then(|| {
            self.ci
                .by_sha
                .iter()
                .map(|(sha, s)| (sha.clone(), s.state))
                .collect()
        });
        let mut said = Vec::new();
        match (self.prs_loaded, &self.pr_error) {
            (Some(at), _) => said.push(format!(
                "pull requests as the tab read them {} ago",
                ago(at.elapsed().as_secs())
            )),
            (None, Some(e)) => said.push(format!("pull requests: {e}")),
            (None, None) => said.push("pull requests: not read".to_owned()),
        }
        match (&builds, &self.ci.error) {
            (Some(_), Some(e)) => said.push(format!("builds partly read: {e}")),
            (Some(_), None) => said.push("builds as the tab last read them".to_owned()),
            (None, _) => said.push("builds: not read".to_owned()),
        }
        (Forge { prs, builds }, said.join(" · "))
    }

    /// Draws the constellation again from what was read, the forge and the disk sizes.
    fn constellation_build(&mut self) {
        let (forge, forge_note) = self.constellation_forge();
        let t = Tokens::default();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.constellation;
        let Some(snapshot) = state.read.ready() else {
            return;
        };
        let model = model::build(snapshot, &forge, crate::git::now());
        let graph = to_graph(&model, state.disk.ready(), snapshot.remote, &t);
        let positions = graph::arrange(&graph, Arrange::Columns);
        let mut provenance = snapshot.provenance.clone();
        if state.disk.loading {
            provenance.scope.push_str(" · disk usage being read");
        }
        // The same nodes keep their picks; others lose them.
        let key = |b: &Option<Built>, i: Option<usize>| {
            i.and_then(|i| {
                b.as_ref()?
                    .model
                    .nodes
                    .get(i)
                    .map(|n| (n.kind, n.key.clone()))
            })
        };
        let (was, was_other) = (
            key(&state.built, state.selected),
            key(&state.built, state.other),
        );
        state.selected = was.and_then(|(k, key)| model.find(k, &key));
        state.other = was_other.and_then(|(k, key)| model.find(k, &key));
        state.built = Some(Built {
            model,
            graph,
            positions,
            provenance,
            forge: forge_note,
        });
    }

    /// Measures the worktrees' folders in the background (those here; not those on another
    /// machine).
    fn constellation_disk(&mut self, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.constellation;
        let Some(snapshot) = state.read.ready() else {
            return;
        };
        if snapshot.remote {
            return;
        }
        let folders: Vec<String> = snapshot
            .worktrees
            .iter()
            .filter(|w| w.prunable.is_none())
            .map(|w| w.path.clone())
            .collect();
        let request = state.disk.start();
        self.insight_then(
            ctx,
            move || {
                folders
                    .into_iter()
                    .map(|f| {
                        let disk = read::disk_usage(std::path::Path::new(&f), read::DISK_ENTRIES);
                        (f, disk)
                    })
                    .collect()
            },
            move |tab, sizes, _| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.constellation.disk.accept(request, Ok(sizes)) {
                    tab.constellation_build();
                }
            },
        );
    }

    pub(super) fn constellation_act(&mut self, act: Act, _ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.constellation;
        let is_branch = |state: &State, i: usize| {
            state
                .built
                .as_ref()
                .and_then(|b| b.model.nodes.get(i))
                .is_some_and(|n| n.kind == Kind::Branch)
        };
        match act {
            Act::Select { node, add } => {
                let pair = add
                    && state
                        .selected
                        .is_some_and(|s| s != node && is_branch(state, s))
                    && is_branch(state, node);
                if pair {
                    state.other = Some(node);
                } else {
                    state.selected = Some(node);
                    state.other = None;
                }
            }
            Act::CompareWith(node) => {
                if state.selected.is_some_and(|s| s != node) && is_branch(state, node) {
                    state.other = Some(node);
                }
            }
            Act::Deselect => {
                state.selected = None;
                state.other = None;
            }
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn constellation_back(&mut self) -> bool {
        let Some(view) = self.insights.as_mut() else {
            return false;
        };
        let state = &mut view.constellation;
        if state.other.take().is_some() {
            return true;
        }
        state.selected.take().is_some()
    }

    pub(super) fn constellation_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        // Stale once a branch it read moved (as far as the tab knows: a tab not loaded yet
        // has no branches to tell by).
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
        let state = &mut view.constellation;
        match common::slot_state(ui, &state.read, "branches and worktrees") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let tips = state
            .read
            .ready()
            .map(|s| s.tips.clone())
            .unwrap_or_default();
        let (Some(built), selected, other) = (state.built.as_mut(), state.selected, state.other)
        else {
            return;
        };
        if !current.is_empty() {
            let moved = model::moved(&tips, |name| current.get(name).cloned());
            built.provenance.stale = (!moved.is_empty()).then(|| {
                let mut names: Vec<&str> = moved.iter().take(3).map(String::as_str).collect();
                let more = moved.len().saturating_sub(3);
                let more = if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                };
                names.sort_unstable();
                format!("{}{more} moved since", names.join(", "))
            });
        }
        let t = Tokens::get(ui.ctx());
        common::provenance(ui, &built.provenance);
        ui.label(RichText::new(&built.forge).size(11.5).color(t.text_faint));
        legend(ui, &t);
        for note in &built.model.notes {
            ui.add(egui::Label::new(RichText::new(note).size(11.5).color(t.text_faint)).wrap());
        }
        if built.model.nodes.is_empty() {
            common::empty(ui, "No branches or worktrees to show.");
            return;
        }
        let details_h = if selected.is_some() { 190.0 } else { 0.0 };
        let room = (ui.available_height() - details_h).max(240.0);
        let width = ui.available_width();
        ui.allocate_ui(vec2(width, room), |ui| {
            if table {
                nodes_table(ui, &built.model, selected, other, acts);
            } else {
                ui.label(
                    RichText::new(
                        "Columns: branches · worktrees · agent tasks · pull requests · builds. \
                         Click a node for what it is; Ctrl or Cmd and a click picks a second \
                         branch to compare.",
                    )
                    .size(11.5)
                    .color(t.text_faint),
                );
                let picked = graph::show(
                    ui,
                    "constellation",
                    &built.graph,
                    &built.positions,
                    selected.map(Pick::Node),
                    (room - 24.0).max(200.0),
                );
                if let Some(Pick::Node(node)) = picked.clicked {
                    let add = ui.input(|i| i.modifiers.command);
                    acts.push(act(Act::Select { node, add }));
                }
            }
        });
        if let Some(i) = selected {
            egui::ScrollArea::vertical()
                .id_salt("constellation-details")
                .auto_shrink([false, true])
                .show(ui, |ui| details(ui, built, i, other, acts));
        }
    }
}

/// `secs` seconds as a short span.
fn ago(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        _ => format!("{}h", secs / 3600),
    }
}
