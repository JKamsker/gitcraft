//! Code evolution heatmap (#175 item 4): commits (or time) across, files or folders down, coloured by how much changed.
//!
//! The history of `HEAD` or a branch is read in the background (`git log --numstat`, a limited
//! number of commits) and the matrix built from it in the background too, again whenever the
//! axis, the grouping or the filter change. A file's cell opens that file's diff in its commit;
//! a folder's cell, or a day with several commits, lists them to choose from.

mod presentation;
use presentation::{boundary_note, controls, grid, legend};

use std::sync::Arc;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::heat_grid::{self, Cell, Layout};
use crate::insights::Provenance;
use crate::insights::evolution::{self as core, Axis, Matrix, Measure, Open, Options, Tally};
use crate::insights::history::{self, History};
use crate::repo::file_view::FileTab;
use crate::repo::{Action, DiffKey, RepoTab};
use crate::theme::Tokens;

/// The history limits offered, and the one read first.
const LIMITS: [usize; 3] = [200, 500, 2000];
const DEFAULT_LIMIT: usize = 500;
/// The row limits offered.
const ROW_LIMITS: [usize; 3] = [50, 200, 1000];
/// The deepest folders offered as rows.
const MAX_DEPTH: usize = 6;

/// A history read and where it came from.
pub(crate) struct Read {
    history: Arc<History>,
    provenance: Provenance,
}

/// A matrix built, with the history it was built from (its cells point into that history).
pub(crate) struct Built {
    history: Arc<History>,
    provenance: Provenance,
    options: Options,
    matrix: Matrix,
    /// The cells that count something, by row and then column: the table's lines.
    cells: Vec<(usize, usize)>,
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
    /// The cell selected, in the matrix's rows and columns.
    selected: Option<(usize, usize)>,
    /// The commits listed to choose from.
    pick: Option<Pick>,
}

impl State {
    fn limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_LIMIT
        } else {
            self.limit
        }
    }
}

/// Commits listed to choose one from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// Those of a cell (a folder, or a file over several commits of a day, week or month).
    Cell(usize, usize),
    /// Those of a day, week or month.
    Column(usize),
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read this branch (`None`: `HEAD`).
    Rev(Option<String>),
    /// Read at most so many commits.
    Limit(usize),
    /// The options changed: build the matrix again.
    Rebuild,
}

impl RepoTab {
    fn evolution_state(&mut self) -> Option<&mut State> {
        self.insights.as_deref_mut().map(|v| &mut v.evolution)
    }

    /// Reads what the view shows (again).
    pub(super) fn evolution_load(&mut self, ctx: &egui::Context) {
        let Some(state) = self.evolution_state() else {
            return;
        };
        let request = state.read.start();
        let rev = state.rev.clone().unwrap_or_else(|| "HEAD".to_owned());
        let limit = state.limit();
        let path = self.path.clone();
        self.insight_then(
            ctx,
            move || {
                history::read(&path, &rev, limit).map(|h| Read {
                    provenance: h.provenance(),
                    history: Arc::new(h),
                })
            },
            move |tab, result, ctx| {
                let Some(state) = tab.evolution_state() else {
                    return;
                };
                if state.read.accept(request, result) {
                    state.selected = None;
                    state.pick = None;
                    tab.evolution_rebuild(ctx);
                }
            },
        );
    }

    /// Builds the matrix of the history read, with the options set, in the background.
    fn evolution_rebuild(&mut self, ctx: &egui::Context) {
        let Some(state) = self.evolution_state() else {
            return;
        };
        let Some(read) = state.read.ready() else {
            return;
        };
        let history = Arc::clone(&read.history);
        let provenance = read.provenance.clone();
        let options = state.options.clone();
        let request = state.built.start();
        self.insight_then(
            ctx,
            move || {
                let matrix = core::build(&history, &options);
                let mut cells: Vec<(usize, usize)> = matrix.cells.keys().copied().collect();
                cells.sort_unstable();
                Ok(Arc::new(Built {
                    history,
                    provenance,
                    options,
                    matrix,
                    cells,
                }))
            },
            move |tab, result, _| {
                let Some(state) = tab.evolution_state() else {
                    return;
                };
                if state.built.accept(request, result) {
                    state.selected = None;
                    state.pick = None;
                }
            },
        );
    }

    pub(super) fn evolution_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.evolution_state() else {
            return;
        };
        match act {
            Act::Rev(rev) => {
                state.rev = rev;
                self.evolution_load(ctx);
            }
            Act::Limit(limit) => {
                state.limit = limit;
                self.evolution_load(ctx);
            }
            Act::Rebuild => self.evolution_rebuild(ctx),
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn evolution_back(&mut self) -> bool {
        let Some(state) = self.evolution_state() else {
            return false;
        };
        if state.pick.take().is_some() {
            return true;
        }
        state.selected.take().is_some()
    }

    /// The cell at (`row`, `column`) of the matrix opened: selected, and its file's diff to
    /// open, or its commits listed.
    pub(super) fn evolution_open(&mut self, row: usize, column: usize) -> Option<Action> {
        let state = self.evolution_state()?;
        let built = state.built.ready().cloned()?;
        state.selected = Some((row, column));
        state.pick = None;
        match built.matrix.open(&built.history, row, column) {
            Open::Diff { hash, path } => Some(Action::OpenDiff(DiffKey::Commit { hash, path })),
            Open::Commits(_) => {
                state.pick = Some(Pick::Cell(row, column));
                None
            }
            Open::Nothing => None,
        }
    }

    /// A column's header opened: its commit selected, or its commits listed.
    fn evolution_open_column(&mut self, column: usize) -> Option<Action> {
        let state = self.evolution_state()?;
        let built = state.built.ready().cloned()?;
        let col = built.matrix.columns.get(column)?;
        if built.options.axis == Axis::Commits {
            let hash = built
                .history
                .commits
                .get(*col.commits.first()?)?
                .hash
                .clone();
            return Some(Action::SelectHash(hash));
        }
        state.pick = Some(Pick::Column(column));
        None
    }

    /// A row's label opened: a file's history at the tip read, or a folder zoomed into.
    fn evolution_open_row(&mut self, row: usize) -> Option<Action> {
        let state = self.evolution_state()?;
        let built = state.built.ready().cloned()?;
        let r = built.matrix.rows.get(row)?;
        if !r.folder {
            return Some(Action::FileView {
                path: r.name.clone(),
                rev: Some(built.history.tip.clone()),
                tab: FileTab::History,
            });
        }
        let depth = built.options.group.unwrap_or(1);
        state.options.filter = if r.name.is_empty() {
            String::new()
        } else {
            format!("{}/", r.name)
        };
        state.options.group = Some((depth + 1).min(MAX_DEPTH));
        Some(Action::Insights(InsightsAct::Evolution(Act::Rebuild)))
    }

    pub(super) fn evolution_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let branches: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let current = tips(&self.refs);
        let Some(view) = self.insights.as_deref_mut() else {
            return;
        };
        let table = view.table;
        let state = &mut view.evolution;
        controls(ui, state, &branches, acts);
        match common::slot_state(ui, &state.read, "the history") {
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
            SlotState::Ready => {}
        }
        let Some(built) = state.built.ready().cloned() else {
            common::loading(ui, "the matrix");
            return;
        };
        let mut provenance = built.provenance.clone();
        if let Some(current) = &current {
            provenance.check_tips(|name| current(name));
        }
        add_partial(&mut provenance, cut_notes(&built.matrix));
        common::provenance(ui, &provenance);
        legend(ui, &built);
        let matrix = &built.matrix;
        if matrix.boundary {
            boundary_note(ui, &built, state.limit(), acts);
        }
        if matrix.rows.is_empty() {
            let text = if built.options.filter.trim().is_empty() {
                "No file changed in the commits read"
            } else {
                "No path changed in the commits read matches the filter"
            };
            common::empty(ui, text);
            return;
        }
        if let Some(pick) = state.pick.clone()
            && !lists::pick(ui, &built, &pick, acts)
        {
            state.pick = None;
        }
        let selected = state.selected;
        let opened = if table {
            lists::table(ui, &built, selected)
        } else {
            grid(ui, &built, selected)
        };
        let action = match opened {
            Some(Opened::Cell(r, c)) => self.evolution_open(r, c),
            Some(Opened::Column(c)) => self.evolution_open_column(c),
            Some(Opened::Row(r)) => self.evolution_open_row(r),
            None => None,
        };
        acts.extend(action);
    }
}

/// What was clicked (or opened with Enter), in the matrix's rows and columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opened {
    Cell(usize, usize),
    Column(usize),
    Row(usize),
}

/// Where each tip name points now, once the branches are read: to tell what was read stale.
pub(super) type Tips = Box<dyn Fn(&str) -> Option<String>>;

pub(super) fn tips(refs: &crate::git::Refs) -> Option<Tips> {
    if refs.head.is_empty() {
        return None;
    }
    let head = refs.head.clone();
    let local: Vec<(String, String)> = refs
        .local
        .iter()
        .map(|b| (b.name.clone(), b.target.clone()))
        .collect();
    Some(Box::new(move |name: &str| {
        if name == "HEAD" {
            return Some(head.clone());
        }
        local
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, sha)| sha.clone())
    }))
}

/// Adds `notes` to why `p` is incomplete.
pub(super) fn add_partial(p: &mut Provenance, notes: Vec<String>) {
    if notes.is_empty() {
        return;
    }
    let mut parts: Vec<String> = p.partial.take().into_iter().collect();
    parts.extend(notes);
    p.partial = Some(parts.join("; "));
}

/// What the matrix left out, said.
fn cut_notes(m: &Matrix) -> Vec<String> {
    let mut notes = Vec::new();
    if m.rows_cut > 0 {
        notes.push(format!(
            "{} more rows changed less and aren't shown (row limit)",
            m.rows_cut
        ));
    }
    if m.columns_cut > 0 {
        notes.push(format!(
            "the oldest {} columns aren't shown (at most {})",
            m.columns_cut,
            core::MAX_BUCKETS
        ));
    }
    notes
}

mod lists;

#[cfg(test)]
mod tests;
