//! Change impact graph (#175 item 1): what refers to the symbols a change touched, at the base
//! and head revisions compared. The change is the selected commit against its parent, the
//! checked-out branch since it left the default branch, or a pull request. Its Rust items are
//! observed; what refers to them is a name match (inferred); files in other languages are
//! unknown. Clicking a node opens the definition or reference at the revision it was read at,
//! never the working tree; clicking an edge lists the lines behind it.

mod presentation;
use presentation::{controls, details, legend, notes};

mod picture;
mod tables;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use egui::{RichText, Sense, Stroke, Ui, vec2};

use self::picture::{Filter, Laid, NodeRef, Scope};
use super::InsightsAct;
use super::common::{self, Mark, Slot, SlotState};
use super::graph::{self, Pick};
use crate::insights::impact::{self as core, Ask, Impact, Reference};
use crate::repo::{Action, DiffKey, RepoTab, WIP_HASH, short};
use crate::theme::Tokens;

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// What the graph is about, as picked; the selected commit (or else the branch) when
    /// `None`.
    ask: Option<Ask>,
    /// What was last read: what the menu marks, whatever the selection is now.
    read_ask: Option<Ask>,
    data: Slot<Arc<Impact>>,
    layout: Slot<Arc<Laid>>,
    filter: Filter,
    /// Laid out by force rather than in columns.
    force: bool,
    selected: Option<Pick>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Read the impact of another change.
    Ask(Ask),
    Filter(Filter),
    Force(bool),
    Select(Option<Pick>),
}

fn act(a: Act) -> Action {
    Action::Insights(InsightsAct::Impact(a))
}

/// Opens a reference's line at the revision it was read at.
fn open_ref(r: &Reference) -> Action {
    Action::BlameAt {
        path: r.path.clone(),
        rev: Some(r.rev.clone()),
        line: r.line as usize,
    }
}

/// What clicking a node opens: a symbol's definition or a site at the revision it was read
/// at, or the diff of a file no analyzer reads.
fn open_node(data: &Impact, node: NodeRef) -> Option<Action> {
    match node {
        NodeRef::Symbol(i) => {
            let span = data.symbols.get(i)?.at()?;
            Some(Action::BlameAt {
                path: span.path.clone(),
                rev: Some(span.rev.clone()),
                line: span.line as usize,
            })
        }
        NodeRef::Site(i) => {
            let site = data.sites.get(i)?;
            Some(Action::BlameAt {
                path: site.path.clone(),
                rev: Some(site.rev.clone()),
                line: site.line as usize,
            })
        }
        NodeRef::Other(i) => Some(file_diff(data, &data.other.get(i)?.path)),
    }
}

/// The change's diff of `path`, from its base to its head.
fn file_diff(data: &Impact, path: &str) -> Action {
    Action::OpenDiff(DiffKey::Range {
        from: data.range.base.clone(),
        to: Some(data.range.head.clone()),
        path: path.to_owned(),
    })
}

impl RepoTab {
    /// What the graph is about unless something else is picked: the pull request the page was
    /// opened over, else the selected commit, else the checked-out branch.
    fn impact_default_ask(&self) -> Ask {
        let opened_over = self.insights.as_ref().and_then(|v| v.pr);
        if let Some(pr) = opened_over.and_then(|n| self.prs.iter().find(|p| p.number == n)) {
            return Ask::Pr {
                number: pr.number,
                head: pr.sha.clone(),
                base: pr.base.clone(),
            };
        }
        match self.detail_hash.as_deref() {
            Some(hash) if hash != WIP_HASH => Ask::Commit(hash.to_owned()),
            _ => Ask::Branch {
                base: self.base_branch_name(),
            },
        }
    }

    /// The changes the graph can be about, for its menu.
    fn impact_subjects(&self) -> Vec<(String, Ask)> {
        let mut list = Vec::new();
        if let Some(hash) = self.detail_hash.as_deref().filter(|h| *h != WIP_HASH) {
            list.push((
                format!("Selected commit {} against its parent", short(hash)),
                Ask::Commit(hash.to_owned()),
            ));
        }
        let base = self.base_branch_name();
        list.push((
            format!(
                "Checked-out branch since it left {}",
                base.as_deref().unwrap_or("the default branch")
            ),
            Ask::Branch { base },
        ));
        for pr in self.prs.iter().take(30) {
            list.push((
                format!("#{} {}", pr.number, graph::elide(&pr.title, 48)),
                Ask::Pr {
                    number: pr.number,
                    head: pr.sha.clone(),
                    base: pr.base.clone(),
                },
            ));
        }
        list
    }

    /// Why what is shown is stale: the branch or pull request it was read from points
    /// elsewhere now.
    fn impact_stale(&self) -> Option<String> {
        let data = self.insights.as_ref()?.impact.data.ready()?;
        let (name, sha) = data.range.tip.as_ref()?;
        let now = if let Some(number) = name.strip_prefix('#') {
            let number: u64 = number.parse().ok()?;
            self.prs.iter().find(|p| p.number == number)?.sha.clone()
        } else if name == "HEAD" {
            self.refs.head.clone()
        } else {
            self.refs
                .local
                .iter()
                .find(|b| &b.name == name)?
                .target
                .clone()
        };
        (!now.is_empty() && &now != sha).then(|| format!("{name} is at {} now", short(&now)))
    }

    /// Reads what the view shows (again).
    pub(super) fn impact_load(&mut self, ctx: &egui::Context) {
        let fallback = self.impact_default_ask();
        let path = self.path.clone();
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        let ask = state.ask.clone().unwrap_or(fallback);
        state.read_ask = Some(ask.clone());
        let request = state.data.start();
        state.selected = None;
        self.insight_then(
            ctx,
            move || {
                core::resolve(&path, &ask)
                    .and_then(|range| core::analyze(&path, &range))
                    .map(Arc::new)
            },
            move |tab, result, ctx| {
                let Some(view) = tab.insights.as_mut() else {
                    return;
                };
                if view.impact.data.accept(request, result) {
                    // The layout and what is picked were of the data before: their places
                    // aren't this data's.
                    view.impact.layout = Slot::default();
                    view.impact.selected = None;
                    view.impact.filter.symbol = None;
                    tab.impact_layout(ctx);
                }
            },
        );
    }

    /// Builds the graph of what was read, as the filters say, in the background.
    fn impact_layout(&mut self, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        let Some(data) = state.data.ready().cloned() else {
            return;
        };
        let (filter, force) = (state.filter.clone(), state.force);
        let request = state.layout.start();
        self.insight_then(
            ctx,
            move || -> Result<Arc<Laid>, String> {
                Ok(Arc::new(picture::build(&data, &filter, force, &t)))
            },
            move |tab, laid, _| {
                if let Some(view) = tab.insights.as_mut() {
                    view.impact.layout.accept(request, laid);
                }
            },
        );
    }

    pub(super) fn impact_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(view) = self.insights.as_mut() else {
            return;
        };
        let state = &mut view.impact;
        match act {
            Act::Ask(ask) => {
                state.ask = Some(ask);
                state.filter = Filter::default();
                self.impact_load(ctx);
            }
            Act::Filter(filter) => {
                state.filter = filter;
                state.selected = None;
                self.impact_layout(ctx);
            }
            Act::Force(force) => {
                state.force = force;
                state.selected = None;
                self.impact_layout(ctx);
            }
            Act::Select(pick) => state.selected = pick,
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn impact_back(&mut self) -> bool {
        let Some(view) = self.insights.as_mut() else {
            return false;
        };
        view.impact.selected.take().is_some()
    }

    pub(super) fn impact_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subjects = self.impact_subjects();
        let stale = self.impact_stale();
        let default_ask = self.impact_default_ask();
        let Some(view) = self.insights.as_deref() else {
            return;
        };
        let (state, table) = (&view.impact, view.table);
        let asked = state
            .read_ask
            .clone()
            .or_else(|| state.ask.clone())
            .unwrap_or(default_ask);
        controls(ui, state, &subjects, &asked, acts);
        match common::slot_state(ui, &state.data, "the change and what refers to it") {
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
        let mut p = data.provenance();
        p.stale = stale;
        common::provenance(ui, &p);
        legend(ui);
        notes(ui, data);
        if data.symbols.is_empty() && data.other.is_empty() {
            common::empty(ui, "The change touches no files.");
            return;
        }
        if data.symbols.is_empty() {
            common::unavailable(
                ui,
                "No Rust items changed",
                "Only Rust is analyzed so far; the files below are shown as unknown.",
            );
        }
        if table {
            tables::show(ui, data, &state.filter, acts);
            return;
        }
        let Some(laid) = state.layout.ready() else {
            common::loading(ui, "the graph");
            return;
        };
        if let Some(pick) = state.selected {
            details(ui, data, laid, pick, &state.filter, acts);
        }
        let left_out: Vec<String> = [
            (laid.hidden_symbols, "less referred-to changed symbols"),
            (laid.hidden_other, "files in other languages"),
            (laid.hidden, "less referred-to sites"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        if !left_out.is_empty() {
            let t = Tokens::get(ui.ctx());
            ui.label(
                RichText::new(format!(
                    "Left out of the picture for room: {}; the tables list them",
                    left_out.join(", ")
                ))
                .size(11.5)
                .color(t.text_faint),
            );
        }
        // Tall enough for its fullest column, scrolled when the area is shorter.
        let tall = (laid.tallest as f32 * 26.0 + 80.0).max(320.0);
        let picked = egui::ScrollArea::vertical()
            .id_salt("impact-graph")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                graph::show(
                    ui,
                    "impact",
                    &laid.graph,
                    &laid.positions,
                    state.selected,
                    tall,
                )
            })
            .inner;
        if let Some(pick) = picked.clicked {
            acts.push(act(Act::Select(Some(pick))));
            if let Pick::Node(i) = pick
                && let Some(&node) = laid.nodes.get(i)
            {
                acts.extend(open_node(data, node));
            }
        }
    }
}
