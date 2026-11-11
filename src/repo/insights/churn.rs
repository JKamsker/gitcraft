//! Code complexity and churn timeline (#175 item 12): size, churn and complexity of files and folders over the commits read, and rising hotspots.
//!
//! The history of `HEAD` or a branch is read in the background, sample commits spread over it,
//! and at each sample what the rows hold is read (`git ls-tree`, `git cat-file --batch`,
//! within a budget) along with the test lane's verdicts, in the background too. Each row draws
//! its size, churn and complexity as sparklines; a point opens the commits of its window.

mod presentation;
use presentation::{controls, day, explain, window};

mod chart;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::evolution::{add_partial, tips};
use crate::insights::Provenance;
use crate::insights::churn::{self as core, Options, Timeline, analyzer};
use crate::insights::evolution::{Axis, bucket, bucket_label};
use crate::insights::history::{self, History};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab, short};
use crate::theme::Tokens;

/// The history limits offered, and the one read first.
const LIMITS: [usize; 3] = [200, 500, 2000];
const DEFAULT_LIMIT: usize = 500;
/// The row limits offered.
const ROW_LIMITS: [usize; 3] = [12, 24, 48];
/// The deepest folders offered as rows.
const MAX_DEPTH: usize = 6;

/// A history read and where it came from.
pub(crate) struct Read {
    history: Arc<History>,
    provenance: Provenance,
}

/// A timeline measured, with the history it was planned from.
pub(crate) struct Built {
    history: Arc<History>,
    provenance: Provenance,
    options: Options,
    timeline: Timeline,
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The branch read; `None`: `HEAD`.
    rev: Option<String>,
    /// At most so many commits read (0: [`DEFAULT_LIMIT`]).
    limit: usize,
    options: Options,
    read: Slot<Read>,
    built: Slot<Arc<Built>>,
    /// The row and sample selected: the commits of that window are listed.
    selected: Option<(usize, usize)>,
    /// The sample the keyboard is on, in the row focused.
    cursor: usize,
    /// The filter was edited since the timeline was last asked for.
    filter_edited: bool,
}

impl State {
    pub(super) fn retire_repository(&mut self) {
        *self = Self {
            rev: self.rev.take(),
            limit: self.limit,
            options: std::mem::take(&mut self.options),
            ..Self::default()
        };
    }

    fn limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_LIMIT
        } else {
            self.limit
        }
    }
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read this branch (`None`: `HEAD`).
    Rev(Option<String>),
    /// Read at most so many commits.
    Limit(usize),
    /// The options changed: plan and measure again.
    Rebuild,
}

impl RepoTab {
    fn churn_state(&mut self) -> Option<&mut State> {
        self.insights.as_deref_mut().map(|v| &mut v.churn)
    }

    /// Reads what the view shows (again).
    pub(super) fn churn_load(&mut self, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        let request = state.read.start();
        let rev = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        let limit = state.limit();
        let path = self.path.clone();
        self.insight_then(
            ctx,
            move || {
                history::read(&path, &rev, limit).map(|h| {
                    let mut provenance = h.provenance();
                    provenance.source =
                        "git log --numstat · git ls-tree · git cat-file · test-lane notes"
                            .to_owned();
                    Read {
                        provenance,
                        history: Arc::new(h),
                    }
                })
            },
            move |tab, result, ctx| {
                let Some(state) = tab.churn_state() else {
                    return;
                };
                if state.read.accept(request, result) {
                    state.selected = None;
                    tab.churn_rebuild(ctx);
                }
            },
        );
    }

    /// Plans the samples of the history read and measures them, in the background.
    fn churn_rebuild(&mut self, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        let Some(read) = state.read.ready() else {
            return;
        };
        let history = Arc::clone(&read.history);
        let provenance = read.provenance.clone();
        let options = state.options.clone();
        let request = state.built.start();
        let path = self.path.clone();
        self.insight_then(
            ctx,
            move || {
                let plan = core::plan(&history, &options);
                let measured = core::measure(&path, &plan)?;
                Ok(Arc::new(Built {
                    timeline: core::assemble(plan, measured),
                    history,
                    provenance,
                    options,
                }))
            },
            move |tab, result, _| {
                let Some(state) = tab.churn_state() else {
                    return;
                };
                if state.built.accept(request, result) {
                    state.selected = None;
                }
            },
        );
    }

    pub(super) fn churn_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.churn_state() else {
            return;
        };
        match act {
            Act::Rev(rev) => {
                state.rev = rev;
                self.churn_load(ctx);
            }
            Act::Limit(limit) => {
                state.limit = limit;
                self.churn_load(ctx);
            }
            Act::Rebuild => self.churn_rebuild(ctx),
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn churn_back(&mut self) -> bool {
        self.churn_state()
            .is_some_and(|state| state.selected.take().is_some())
    }

    /// Selects sample `k` of row `row`: the commits of its window are listed.
    #[cfg(test)]
    pub(super) fn churn_select(&mut self, row: usize, k: usize) -> bool {
        let Some(state) = self.churn_state() else {
            return false;
        };
        let Some(built) = state.built.ready() else {
            return false;
        };
        let ok = built
            .timeline
            .rows
            .get(row)
            .is_some_and(|r| k < r.points.len());
        if ok {
            state.selected = Some((row, k));
            state.cursor = k;
        }
        ok
    }

    pub(super) fn churn_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let branches: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let current = tips(&self.refs);
        let Some(view) = self.insights.as_deref_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.churn;
        controls(ui, state, &branches, acts);
        match common::slot_state(ui, &state.read, "the history") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        match common::slot_state(ui, &state.built, "sizes and complexity at the samples") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Churn(Act::Rebuild)));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(built) = state.built.ready().cloned() else {
            return;
        };
        let mut provenance = built.provenance.clone();
        if let Some(current) = &current {
            provenance.check_tips(|name| current(name));
        }
        add_partial(&mut provenance, built.timeline.notes.clone());
        common::provenance(ui, &provenance);
        explain(ui, &built);
        let t = &built.timeline;
        if t.rows.is_empty() {
            let text = if built.options.filter.trim().is_empty() {
                "No file changed in the commits read"
            } else {
                "No path changed in the commits read matches the filter"
            };
            common::empty(ui, text);
            return;
        }
        if let Some((r, k)) = state.selected
            && !window(ui, &built, r, k, acts)
        {
            state.selected = None;
        }
        let picked = if table {
            chart::table(ui, &built, state.selected)
        } else {
            chart::rows(ui, &built, state.selected, &mut state.cursor)
        };
        if let Some((r, k)) = picked {
            state.selected = Some((r, k));
            state.cursor = k;
        }
    }
}
