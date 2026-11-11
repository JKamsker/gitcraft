//! Test coverage relationship map (#175 item 11): what a coverage report says of the lines a
//! change touched (the working tree against HEAD, or a commit against its parent), taken only
//! when the report can be shown to be of that revision (a `<report>.sha` beside it naming the
//! commit). Otherwise what it says stays unknown, never uncovered. Tests appear only when the
//! report records them (lcov `TN:` sections); the test lane's results for the commit appear
//! beside them. A hunk opens its diff, a file the file at the revision, a test what it ran.

mod presentation;
use presentation::{controls, details, lane, legend, trust, uncovered};

mod picture;
mod tables;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Ui};

use self::picture::{Laid, NodeRef};
use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::graph::{self, Pick};
use crate::insights::Evidence;
use crate::insights::coverage::{self as core, Ask, Coverage, HunkCov, LineCov};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab, WIP_HASH, short};
use crate::theme::Tokens;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The lines looked at, as picked; the working tree when `None`.
    ask: Option<Ask>,
    /// The report picked or typed; the first found in the usual places when `None`.
    report: Option<String>,
    /// What is typed in the report's field.
    typed: String,
    data: Slot<Arc<Coverage>>,
    layout: Slot<Arc<Laid>>,
    selected: Option<Pick>,
    /// The test whose hunks stand out.
    test: Option<usize>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    Ask(Ask),
    /// Read this report (`None`: the first found).
    Report(Option<String>),
    Select(Option<Pick>),
    Test(Option<usize>),
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Coverage(a))
}

/// Opens a hunk where it is: the working tree's diff against HEAD, or the commit's diff of
/// those lines.
fn open_hunk(data: &Coverage, file: usize, hunk: usize) -> Option<Action> {
    let f = data.files.get(file)?;
    let h = f.hunks.get(hunk)?;
    Some(match data.ask {
        Ask::WorkTree => Action::OpenDiff(DiffKey::Range {
            from: data.sha.clone(),
            to: None,
            path: f.path.clone(),
        }),
        Ask::Commit(_) => Action::OpenDiffOfLines {
            hash: data.sha.clone(),
            path: f.path.clone(),
            lines: vec![h.span()],
        },
    })
}

/// What clicking a node opens or picks.
fn open_node(data: &Coverage, node: NodeRef, test: Option<usize>) -> Vec<Action> {
    match node {
        NodeRef::File(f) => data
            .files
            .get(f)
            .map(|file| Action::FileView {
                path: file.path.clone(),
                rev: match data.ask {
                    Ask::WorkTree => None,
                    Ask::Commit(_) => Some(data.sha.clone()),
                },
                tab: FileTab::File,
            })
            .into_iter()
            .collect(),
        NodeRef::Hunk(f, h) => open_hunk(data, f, h).into_iter().collect(),
        NodeRef::Test(t) => vec![act(Act::Test((test != Some(t)).then_some(t)))],
        NodeRef::Lane(_) => vec![
            Action::SelectHash(data.sha.clone()),
            Action::TestLane(crate::repo::test_lane::LaneAct::ShowOutput(
                data.tree.clone(),
            )),
        ],
    }
}

impl RepoTab {
    /// The lines the map can be about, for its menu.
    fn coverage_subjects(&self) -> Vec<(String, Ask)> {
        let mut list = vec![("Working tree against HEAD".to_owned(), Ask::WorkTree)];
        if let Some(hash) = self.detail_hash.as_deref().filter(|h| *h != WIP_HASH) {
            list.push((
                format!("Selected commit {} against its parent", short(hash)),
                Ask::Commit(hash.to_owned()),
            ));
        }
        list.push((
            "HEAD against its parent".to_owned(),
            Ask::Commit("HEAD".to_owned()),
        ));
        list
    }

    /// Reads what the view shows (again).
    pub(super) fn coverage_load(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        let ask = state.ask.clone().unwrap_or(Ask::WorkTree);
        let report = state.report.clone();
        let request = state.data.start();
        state.selected = None;
        state.test = None;
        self.insight_then(
            ctx,
            move || core::analyze(&path, &ask, report.as_deref()).map(Arc::new),
            move |tab, result, ctx| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.coverage.data.accept(request, result) {
                    // The layout and what is picked were of the data before: their places
                    // aren't this data's.
                    view.coverage.layout = Slot::default();
                    view.coverage.selected = None;
                    tab.coverage_layout(ctx);
                }
            },
        );
    }

    /// Builds the map of what was read, in the background.
    fn coverage_layout(&mut self, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        let Some(data) = state.data.ready().cloned() else {
            return;
        };
        let test = state.test;
        let request = state.layout.start();
        self.insight_then(
            ctx,
            move || -> Result<Arc<Laid>, String> { Ok(Arc::new(picture::build(&data, test, &t))) },
            move |tab, laid, _| {
                if let Some(view) = tab.insights.as_mut() {
                    view.coverage.layout.accept(request, laid);
                }
            },
        );
    }

    pub(super) fn coverage_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.coverage;
        match act {
            Act::Ask(ask) => {
                state.ask = Some(ask);
                self.coverage_load(ctx);
            }
            Act::Report(report) => {
                state.report = report;
                self.coverage_load(ctx);
            }
            Act::Select(pick) => state.selected = pick,
            Act::Test(test) => {
                state.test = test;
                self.coverage_layout(ctx);
            }
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn coverage_back(&mut self) -> bool {
        let Some(view) = self.insights.as_mut() else {
            return false;
        };
        let state = &mut view.coverage;
        if state.selected.take().is_some() {
            return true;
        }
        if state.test.take().is_some() {
            return true;
        }
        false
    }

    pub(super) fn coverage_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subjects = self.coverage_subjects();
        let Some(view) = self.insights.as_deref_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.coverage;
        controls(ui, state, &subjects, acts);
        let state = &view.coverage;
        match common::slot_state(ui, &state.data, "the changed lines and the coverage report") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(data) = state.data.ready() else {
            return;
        };
        common::provenance(ui, &data.provenance());
        trust(ui, data);
        legend(ui);
        let counts = data.counts();
        let t = Tokens::get(ui.ctx());
        if data.files.is_empty() {
            common::empty(
                ui,
                "No changed lines here: pick a commit above (HEAD against its parent).",
            );
            return;
        }
        ui.label(
            RichText::new(format!("Changed lines: {}", picture::counts_text(counts)))
                .color(t.text_dim),
        );
        uncovered(ui, data, acts);
        lane(ui, data, acts);
        if table {
            tables::show(ui, data, state.test, acts);
            return;
        }
        let Some(laid) = state.layout.ready() else {
            common::loading(ui, "the map");
            return;
        };
        if let Some(pick) = state.selected {
            details(ui, data, laid, pick, acts);
        }
        if laid.hidden > 0 {
            ui.label(
                RichText::new(format!(
                    "{} more hunks are left out of the picture; the table lists them",
                    laid.hidden
                ))
                .size(11.5)
                .color(t.text_faint),
            );
        }
        let picked = graph::show(
            ui,
            "coverage",
            &laid.graph,
            &laid.positions,
            state.selected,
            300.0,
        );
        if let Some(pick) = picked.clicked {
            acts.push(act(Act::Select(Some(pick))));
            if let Pick::Node(i) = pick
                && let Some(&node) = laid.nodes.get(i)
            {
                acts.extend(open_node(data, node, state.test));
            }
        }
    }
}
