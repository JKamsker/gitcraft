//! Change-coupling network (#175 item 5): files that change together, weighted by how often, and those a change leaves out.

mod model;
use model::{build, label, network, percent, read};

mod parts;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{Pos2, Ui};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use super::graph::{self, Arrange, Edge, Graph, Node, NodeShape, Pick};
use crate::insights::coupling::{self, Coupling, LeftOut, Options};
use crate::insights::history::{self, History};
use crate::insights::{Evidence, Provenance};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, RepoTab, short};
use crate::theme::Tokens;
use parts::{controls, left_out_list, legend, pair_commits, pairs_table, summary, unit_details};

/// How many commits can be read.
const LIMITS: [usize; 5] = [200, 500, 1000, 2000, 5000];
const DEFAULT_LIMIT: usize = 1000;
/// How far back commits are counted: a label and seconds.
const RANGES: [(&str, Option<i64>); 5] = [
    ("All read", None),
    ("Last 30 days", Some(30 * 86_400)),
    ("Last 90 days", Some(90 * 86_400)),
    ("Last year", Some(365 * 86_400)),
    ("Last 2 years", Some(2 * 365 * 86_400)),
];
const TOPS: [usize; 5] = [20, 40, 80, 120, 150];
const BULKS: [usize; 4] = [20, 40, 100, 500];

/// Where the files of the change whose left-out files are looked for come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Change {
    #[default]
    None,
    /// The working tree's changes, staged or not.
    Work,
    /// The commit selected in the graph.
    Commit,
}

/// What a change leaves out, worked out for (an analysis, the change's files).
type LeftCache = Option<((u64, Vec<String>), Vec<LeftOut>)>;

/// What a read gives: the analysis of a history, and where its network's nodes go.
pub(crate) struct Model {
    /// Numbers the analyses, so what is worked out of one is known to be of it.
    id: u64,
    history: Arc<History>,
    coupling: Coupling,
    /// The unit each node of the network is.
    nodes: Vec<usize>,
    positions: Vec<Pos2>,
    provenance: Provenance,
}

/// What the view keeps.
pub(crate) struct State {
    limit: usize,
    options: Options,
    /// The folder filter as typed, applied with Enter.
    filter: String,
    /// The range picked, an index into [`RANGES`].
    range: usize,
    read: Slot<Model>,
    /// The pair shown, an index into [`Coupling::pairs`].
    pair: Option<usize>,
    /// The unit shown, an index into [`Coupling::units`].
    unit: Option<usize>,
    change: Change,
    /// The files of a commit not in the history read: (commit, files).
    commit_files: Slot<(String, Vec<String>)>,
    /// The commit whose files were last asked for.
    commit_asked: Option<String>,
    /// What the change leaves out, for (model, files).
    left: LeftCache,
}

impl Default for State {
    fn default() -> Self {
        Self {
            limit: DEFAULT_LIMIT,
            options: Options::default(),
            filter: String::new(),
            range: 0,
            read: Slot::default(),
            pair: None,
            unit: None,
            change: Change::None,
            commit_files: Slot::default(),
            commit_asked: None,
            left: None,
        }
    }
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read this many commits.
    Limit(usize),
    /// Count again with these options.
    Options(Options),
    /// Show a pair and the commits behind it (`None`: none).
    Pair(Option<usize>),
    /// Show a unit and what it changes with.
    Unit(Option<usize>),
    /// Look for what this change leaves out.
    Change(Change),
}

impl RepoTab {
    /// Reads what the view shows (again).
    pub(super) fn coupling_load(&mut self, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.coupling) else {
            return;
        };
        let request = state.read.start();
        let (limit, options) = (state.limit, state.options.clone());
        let repo = self.path.clone();
        self.insight_then(
            ctx,
            move || read(&repo, limit, &options),
            move |tab, found, _| tab.coupling_accept(request, found),
        );
    }

    fn coupling_accept(&mut self, request: u64, found: Result<Model, String>) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.coupling) else {
            return;
        };
        if state.read.accept(request, found) {
            state.pair = None;
            state.unit = None;
            state.left = None;
        }
    }

    pub(super) fn coupling_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.coupling) else {
            return;
        };
        match act {
            Act::Limit(limit) => {
                state.limit = limit;
                self.coupling_load(ctx);
            }
            Act::Options(options) => {
                state.options = options.clone();
                // The history shown is counted again, unless another is being read (a new
                // limit): then that one is read with these options.
                let (loading, limit) = (state.read.loading, state.limit);
                let Some(history) = state
                    .read
                    .ready()
                    .filter(|m| !loading && m.history.limit == limit)
                    .map(|m| Arc::clone(&m.history))
                else {
                    self.coupling_load(ctx);
                    return;
                };
                let request = state.read.start();
                self.insight_then(
                    ctx,
                    move || Ok(build(history, &options)),
                    move |tab, found, _| tab.coupling_accept(request, found),
                );
            }
            Act::Pair(pair) => {
                state.pair = pair;
                state.unit = None;
            }
            Act::Unit(unit) => {
                state.unit = unit;
                state.pair = None;
            }
            Act::Change(change) => {
                state.change = change;
                state.left = None;
            }
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn coupling_back(&mut self) -> bool {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.coupling) else {
            return false;
        };
        if state.pair.is_some() || state.unit.is_some() {
            state.pair = None;
            state.unit = None;
            return true;
        }
        if state.change != Change::None {
            state.change = Change::None;
            return true;
        }
        false
    }

    /// The files of the change looked at, when they are known: (what it is, its files).
    fn coupling_change(&mut self, ctx: &egui::Context) -> Option<(String, Vec<String>)> {
        let change = self.insights.as_ref()?.coupling.change;
        match change {
            Change::None => None,
            Change::Work => {
                let mut files: Vec<String> = (self.work.staged.iter())
                    .chain(&self.work.unstaged)
                    .chain(&self.work.conflicted)
                    .map(|f| f.path.clone())
                    .collect();
                files.sort();
                files.dedup();
                Some(("the working tree's changes".to_owned(), files))
            }
            Change::Commit => {
                let hash = self.detail_hash.clone()?;
                let what = format!("commit {}", short(&hash));
                if let Some(detail) = self.detail.as_ref().filter(|d| d.hash == hash) {
                    return Some((what, detail.files.iter().map(|f| f.path.clone()).collect()));
                }
                let state = &mut self.insights.as_mut()?.coupling;
                if let Some(c) = state
                    .read
                    .ready()
                    .and_then(|m| m.history.commits.iter().find(|c| c.hash == hash))
                {
                    return Some((what, c.files.iter().map(|f| f.path.clone()).collect()));
                }
                match state.commit_files.ready() {
                    Some((of, files)) if *of == hash => Some((what, files.clone())),
                    _ => {
                        if state.commit_asked.as_deref() != Some(hash.as_str()) {
                            state.commit_asked = Some(hash.clone());
                            let request = state.commit_files.start();
                            let repo = self.path.clone();
                            let of = hash.clone();
                            self.insight_then(
                                ctx,
                                move || {
                                    crate::git::commit_detail(&repo, &of).map(|d| {
                                        (of, d.files.into_iter().map(|f| f.path).collect())
                                    })
                                },
                                move |tab, found, _| {
                                    if let Some(v) = tab.insights.as_mut() {
                                        v.coupling.commit_files.accept(request, found);
                                    }
                                },
                            );
                        }
                        None
                    }
                }
            }
        }
    }

    pub(super) fn coupling_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ctx = ui.ctx().clone();
        let change = self.coupling_change(&ctx);
        let head = self.refs.head.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.coupling;
        controls(ui, state, acts);
        match common::slot_state(ui, &state.read, "the history") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let counting = state.read.loading;
        let Some(model) = state.read.ready_mut() else {
            return;
        };
        if !head.is_empty() {
            model
                .provenance
                .check_tips(|name| (name == "HEAD").then(|| head.clone()));
        }
        common::provenance(ui, &model.provenance);
        let model = &*model;
        let c = &model.coupling;
        summary(ui, model, counting);
        if model.history.commits.is_empty() {
            common::empty(ui, "There are no commits to read yet.");
            return;
        }
        if c.counted == 0 {
            common::empty(
                ui,
                "No commit is counted: every commit read is a merge, a bulk change, outside \
                 the folder or older than the range.",
            );
            return;
        }
        // What the change leaves out, worked out once for the analysis and the files.
        let left: Vec<LeftOut> = match &change {
            Some((_, files)) => {
                let key = (model.id, files.clone());
                match &state.left {
                    Some((k, list)) if *k == key => list.clone(),
                    _ => {
                        let list = coupling::left_out(c, files);
                        state.left = Some((key, list.clone()));
                        list
                    }
                }
            }
            None => Vec::new(),
        };
        let changed: Vec<usize> = change
            .as_ref()
            .map(|(_, files)| {
                let mut units: Vec<usize> = files
                    .iter()
                    .filter_map(|f| c.unit_of(f))
                    .filter_map(|n| c.unit_index(n))
                    .collect();
                units.sort_unstable();
                units.dedup();
                units
            })
            .unwrap_or_default();
        let left_units: Vec<usize> = left.iter().map(|l| l.unit).collect();
        let t = Tokens::get(ui.ctx());
        legend(ui, &t, change.is_some());
        if c.pairs.is_empty() {
            common::empty(
                ui,
                &format!(
                    "No two {} changed together in {} or more commits: lower the minimum, or \
                     read more history.",
                    if c.options.depth.is_some() {
                        "folders"
                    } else {
                        "files"
                    },
                    c.options.min_support
                ),
            );
        } else if !table {
            let g = network(c, &model.nodes, &t, &changed, &left_units);
            let selected = match (state.pair, state.unit) {
                (Some(p), _) if p < c.shown => Some(Pick::Edge(p)),
                (_, Some(u)) => model.nodes.binary_search(&u).ok().map(Pick::Node),
                _ => None,
            };
            let height = (ui.available_height() * 0.6).max(260.0);
            let picked = ui
                .allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                    graph::show(ui, "coupling", &g, &model.positions, selected, height)
                })
                .inner;
            match picked.clicked {
                Some(Pick::Edge(i)) => {
                    acts.push(act(Act::Pair(Some(i))));
                }
                Some(Pick::Node(i)) => {
                    let unit = model.nodes[i];
                    acts.push(act(Act::Unit(Some(unit))));
                    if c.options.depth.is_none() {
                        acts.push(Action::FileView {
                            path: c.units[unit].name.clone(),
                            rev: Some(model.history.tip.clone()),
                            tab: FileTab::History,
                        });
                    }
                }
                None => {}
            }
        }
        egui::ScrollArea::vertical()
            .id_salt("coupling-details")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if let Some((what, files)) = &change {
                    left_out_list(ui, c, what, files, &left, acts);
                } else if state.change == Change::Commit {
                    if state.commit_files.loading {
                        common::loading(ui, "the commit's files");
                    } else if let Some(e) = state.commit_files.error() {
                        common::empty(ui, &format!("Couldn't read the commit's files: {e}"));
                    } else {
                        common::empty(ui, "Select a commit to see what it leaves out.");
                    }
                }
                if let Some(p) = state.pair.and_then(|p| c.pairs.get(p)) {
                    pair_commits(ui, model, p, acts);
                } else if let Some(u) = state.unit.filter(|&u| u < c.units.len()) {
                    unit_details(ui, model, u, acts);
                }
                if table && !c.pairs.is_empty() {
                    pairs_table(ui, c, state.pair, acts);
                }
            });
    }
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Coupling(a))
}
