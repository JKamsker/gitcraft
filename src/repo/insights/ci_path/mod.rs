//! CI critical path and regression explorer (#175 item 2): a run's jobs and steps on a time line
//! from the run's start, their queueing apart from their running, the dependencies between
//! them and the critical path through them, against an earlier successful run of the same
//! workflow. A bar opens the run in the CI view, the job's or step's log on the forge, and the
//! commit.

mod controls;
mod detail;
mod diagnostic_view;
mod reads;
mod waterfall;

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use crate::ci::timing::history::RunKey;
use crate::ci::{self, CheckState, Project, Run};
use crate::insights::ci_path::{self as analysis, Baseline, Explorer};
use crate::repo::{Action, RepoTab, WIP_HASH, short};
use crate::theme::Tokens;

/// The runs of the commit the view is about, each with the project it was read from.
#[derive(Debug, Default)]
pub(crate) struct Runs {
    list: Vec<(Project, Run)>,
    /// What the projects that couldn't be read said.
    errors: Vec<String>,
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The commit the view is about: its name (`HEAD`, `selected commit`) and id.
    subject: Option<(String, String)>,
    /// What the view needs and doesn't have: a title, and what would make it available.
    unavailable: Option<(String, String)>,
    runs: Slot<Runs>,
    explorer: Slot<Explorer>,
    /// The run shown, by its project and id.
    run: Option<(Project, u64)>,
    /// Once resolved, refresh keeps this exact attempt rather than following reruns.
    pinned: Option<RunKey>,
    workflow: Option<u64>,
    baseline: Baseline,
    /// The job picked, by index.
    selected: Option<usize>,
    /// The jobs whose steps are shown.
    expanded: HashSet<usize>,
}

impl State {
    fn accepts(&self, explorer: &Explorer) -> bool {
        self.run
            .as_ref()
            .is_none_or(|(project, id)| *project == explorer.project && *id == explorer.run.info.id)
            && self
                .subject
                .as_ref()
                .is_none_or(|(_, sha)| !sha.is_empty() && *sha == explorer.run.info.sha)
            && self
                .pinned
                .is_none_or(|key| key == RunKey::of(&explorer.run.info))
            && self
                .workflow
                .is_none_or(|id| Some(id) == explorer.run.info.workflow_id)
            && match (self.baseline, &explorer.compared) {
                (Baseline::Attempt(key), Some(compared)) => key == RunKey::of(&compared.run.info),
                _ => true,
            }
    }
    fn unavailable(&mut self, title: &str, why: String) {
        let subject = self.subject.take();
        *self = Self::default();
        self.subject = subject;
        self.unavailable = Some((title.to_owned(), why));
    }
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Show this run of the commit (an index into its runs).
    PickRun(usize),
    PickBaseline(Baseline),
    PickAttempt(RunKey),
    /// Pick a job, or none.
    Select(Option<usize>),
    /// Show a job's steps, or hide them.
    Expand(usize),
    /// Read the run shown again.
    ReloadRun,
}

/// Whether `error` says the forge's tool is missing or not signed in: what is needed, rather
/// than a failure to retry.
pub(super) fn tool_problem(error: &str) -> bool {
    let e = error.to_lowercase();
    e.contains("isn't installed") || e.contains("isn't signed in") || e.contains("auth login")
}

fn act(act: Act) -> Action {
    Action::Insights(InsightsAct::CiPath(act))
}

impl RepoTab {
    /// The commit the view is about: the selected one, else HEAD.
    pub(super) fn ci_path_subject(&self) -> Option<(String, String)> {
        if let Some(hash) = self
            .detail_hash
            .as_ref()
            .filter(|h| !h.is_empty() && h.as_str() != WIP_HASH)
        {
            return Some(("selected commit".to_owned(), hash.clone()));
        }
        (!self.refs.head.is_empty()).then(|| ("HEAD".to_owned(), self.refs.head.clone()))
    }

    pub(super) fn ci_path_act(&mut self, act: Act, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        match act {
            Act::PickRun(i) => {
                let Some(pick) = state.runs.ready().and_then(|r| r.list.get(i)) else {
                    return;
                };
                let pick = Some((pick.0.clone(), pick.1.id));
                let pinned =
                    state
                        .runs
                        .ready()
                        .and_then(|runs| runs.list.get(i))
                        .map(|(_, run)| RunKey {
                            run: run.id,
                            attempt: run.attempt.max(1),
                        });
                if pick != state.run {
                    state.run = pick;
                    state.pinned = pinned;
                    state.workflow = None;
                    state.selected = None;
                    state.expanded.clear();
                    state.baseline = Baseline::Latest;
                    self.ci_path_read_run(ctx);
                }
            }
            Act::PickBaseline(baseline) => {
                if baseline != state.baseline {
                    state.baseline = baseline;
                    self.ci_path_read_run(ctx);
                }
            }
            Act::PickAttempt(key) => {
                let valid = state.explorer.ready().is_some_and(|explorer| {
                    key.run == explorer.run.info.id
                        && explorer
                            .history
                            .runs
                            .iter()
                            .any(|run| RunKey::of(&run.info) == key)
                });
                if valid && state.pinned != Some(key) {
                    state.pinned = Some(key);
                    state.baseline = Baseline::Latest;
                    state.selected = None;
                    state.expanded.clear();
                    self.ci_path_read_run(ctx);
                }
            }
            Act::Select(job) => state.selected = job,
            Act::Expand(job) => {
                if !state.expanded.remove(&job) {
                    state.expanded.insert(job);
                }
            }
            Act::ReloadRun => self.ci_path_read_run(ctx),
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn ci_path_back(&mut self) -> bool {
        self.insights
            .as_mut()
            .and_then(|v| v.ci_path.selected.take())
            .is_some()
    }

    pub(super) fn ci_path_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subject_now = self.ci_path_subject();
        let Some(view) = self.insights.as_deref() else {
            return;
        };
        let (state, table) = (&view.ci_path, view.table);
        if let Some((title, why)) = &state.unavailable {
            common::unavailable(ui, title, why);
            return;
        }
        match common::slot_state(ui, &state.runs, "the runs of the commit") {
            SlotState::Ready => {}
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
        }
        let Some(runs) = state.runs.ready() else {
            return;
        };
        let sha = state.subject.as_ref().map_or("", |(_, s)| s.as_str());
        if runs.list.is_empty() {
            let mut text = format!("No CI runs of {} were found on the forge.", short(sha));
            if !runs.errors.is_empty() {
                text.push_str(&format!(" Not read: {}", runs.errors.join("; ")));
            }
            common::empty(ui, &text);
            return;
        }
        controls::header(ui, state, runs, acts);
        match common::slot_state(ui, &state.explorer, "the run") {
            SlotState::Ready => {}
            SlotState::Retry => {
                acts.push(act(Act::ReloadRun));
                return;
            }
            SlotState::Waiting => return,
        }
        let Some(explorer) = state.explorer.ready() else {
            return;
        };
        let mut provenance = explorer.provenance.clone();
        provenance.check_tips(|name| match &subject_now {
            Some((now, sha)) if now == name => Some(sha.clone()),
            _ => None,
        });
        if provenance.stale.is_some() {
            provenance.stale = Some("The commit the view is about changed".to_owned());
        }
        if !runs.errors.is_empty() {
            let why = format!("Not every project answered: {}", runs.errors.join("; "));
            provenance.partial = Some(match provenance.partial.take() {
                Some(p) => format!("{p}; {why}"),
                None => why,
            });
        }
        common::provenance(ui, &provenance);
        legend(ui, explorer.compared.is_some());
        detail::summary(ui, explorer);
        diagnostic_view::summary(ui, explorer);
        egui::ScrollArea::vertical()
            .id_salt("ci-path-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if table {
                    detail::table(ui, explorer, state.selected, acts);
                } else {
                    waterfall::show(ui, explorer, state.selected, &state.expanded, acts);
                }
                if let Some(job) = state.selected {
                    detail::job(ui, explorer, job, state.expanded.contains(&job), acts);
                    diagnostic_view::job(ui, explorer, job);
                } else {
                    ui.add_space(6.0);
                    common::empty(
                        ui,
                        "Pick a job (click it, or Tab to it and press Enter) for its steps, \
                         what it waited for and its log.",
                    );
                }
            });
    }
}

/// What a state is called, in words.
fn state_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Running => "running",
        CheckState::Skipped => "skipped",
    }
}

fn legend(ui: &mut Ui, compared: bool) {
    let t = Tokens::get(ui.ctx());
    let mut entries = vec![
        (common::Mark::Fill(t.accent), "running"),
        (
            common::Mark::Hatched(t.warning),
            "queued (waiting for a runner)",
        ),
        (common::Mark::Glyph("!", t.danger), "failed"),
        (common::Mark::Glyph("?", t.text_dim), "duration unknown"),
        (common::Mark::Glyph("C", t.text), "on the critical path"),
        (
            common::evidence_line(crate::insights::Evidence::Observed, t.text_dim),
            "dependency from the workflow file",
        ),
        (
            common::evidence_line(crate::insights::Evidence::Inferred, t.text_dim),
            "dependency inferred",
        ),
    ];
    if compared {
        entries.push((common::Mark::Hollow(t.text_faint), "the earlier run"));
        entries.push((common::Mark::Glyph("+", t.warning), "newly slow"));
    }
    common::legend(ui, &entries);
}
