//! CI critical path and regression explorer (#175 item 2): a run's jobs and steps on a time line
//! from the run's start, their queueing apart from their running, the dependencies between
//! them and the critical path through them, against an earlier successful run of the same
//! workflow. A bar opens the run in the CI view, the job's or step's log on the forge, and the
//! commit.

mod detail;
mod waterfall;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use egui::{RichText, Ui};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
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
    baseline: Baseline,
    /// The job picked, by index.
    selected: Option<usize>,
    /// The jobs whose steps are shown.
    expanded: HashSet<usize>,
}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// Show this run of the commit (an index into its runs).
    PickRun(usize),
    PickBaseline(Baseline),
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

    /// Reads what the view shows (again): the runs of the commit, then the one shown.
    pub(super) fn ci_path_load(&mut self, ctx: &egui::Context) {
        let subject = self.ci_path_subject();
        let projects = subject
            .as_ref()
            .map(|(_, sha)| self.ci_projects_for(sha))
            .unwrap_or_default();
        let forge = self.forge();
        let path = self.path.clone();
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        state.subject = subject.clone();
        state.unavailable = None;
        let Some((_, sha)) = subject else {
            state.unavailable = Some((
                "No commit".to_owned(),
                "Select a commit, or open a repository with one, to see its CI runs.".to_owned(),
            ));
            return;
        };
        if projects.is_empty() {
            let why = match forge {
                None => "This repository has no remote on GitHub or GitLab: runs are read \
                         through gh or glab from the repository's forge."
                    .to_owned(),
                Some(forge) => format!(
                    "gitcraft reads CI runs from GitHub and GitLab only; {} isn't supported yet.",
                    forge.name()
                ),
            };
            state.unavailable = Some(("No CI to read".to_owned(), why));
            return;
        }
        let request = state.runs.start();
        self.insight_then(
            ctx,
            move || {
                let mut runs = Runs::default();
                let mut answered = false;
                for project in projects {
                    match ci::runs(&path, &project, &sha) {
                        Ok(list) => {
                            answered = true;
                            runs.list
                                .extend(list.into_iter().map(|run| (project.clone(), run)));
                        }
                        Err(e) => runs.errors.push(format!("{}: {e}", project.path)),
                    }
                }
                if answered {
                    Ok(runs)
                } else {
                    Err(runs.errors.join("; "))
                }
            },
            move |tab, result: Result<Runs, String>, ctx| tab.ci_path_runs(request, result, ctx),
        );
    }

    /// The runs of the commit arrived: the one shown before stays, else the newest.
    fn ci_path_runs(&mut self, request: u64, result: Result<Runs, String>, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        if !state.runs.accept(request, result) {
            return;
        }
        state.unavailable = state
            .runs
            .error()
            .filter(|error| tool_problem(error))
            .map(|error| ("The forge's tool is needed".to_owned(), error.to_owned()));
        if state.unavailable.is_some() {
            return;
        }
        let Some(runs) = state.runs.ready() else {
            return;
        };
        let keep = runs
            .list
            .iter()
            .find(|(p, r)| state.run.as_ref() == Some(&(p.clone(), r.id)));
        let pick = keep
            .or_else(|| runs.list.first())
            .map(|(p, r)| (p.clone(), r.id));
        if pick != state.run {
            state.selected = None;
            state.expanded.clear();
        }
        state.run = pick;
        if state.run.is_some() {
            self.ci_path_read_run(ctx);
        }
    }

    /// Reads the run shown, its dependencies and the earlier run it is compared with.
    fn ci_path_read_run(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        let Some(state) = self.insights.as_mut().map(|v| &mut v.ci_path) else {
            return;
        };
        let (Some((project, run)), Some((name, sha))) = (state.run.clone(), state.subject.clone())
        else {
            return;
        };
        let baseline = state.baseline;
        let request = state.explorer.start();
        self.insight_then(
            ctx,
            move || analysis::read(&path, &project, run, baseline, (&name, &sha)),
            move |tab, result: Result<Explorer, String>, _ctx| {
                let Some(state) = tab.insights.as_mut().map(|v| &mut v.ci_path) else {
                    return;
                };
                if state.explorer.accept(request, result)
                    && let Some(explorer) = state.explorer.ready()
                {
                    let jobs = explorer.run.jobs.len();
                    state.selected = state.selected.filter(|&i| i < jobs);
                    state.expanded.retain(|&i| i < jobs);
                }
            },
        );
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
                if pick != state.run {
                    state.run = pick;
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
        header(ui, state, runs, acts);
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

/// The run picker, the earlier run picker and the commit.
fn header(ui: &mut Ui, state: &State, runs: &Runs, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if let Some((name, sha)) = &state.subject {
            ui.label(RichText::new(format!("{name}:")).color(t.text_dim));
            if ui
                .link(short(sha))
                .on_hover_text("Select this commit in the graph")
                .clicked()
            {
                acts.push(Action::SelectHash(sha.clone()));
            }
        }
        ui.add_space(8.0);
        let shown = runs
            .list
            .iter()
            .position(|(p, r)| state.run.as_ref() == Some(&(p.clone(), r.id)));
        let label = |(p, r): &(Project, Run)| {
            format!(
                "{} #{} · {} · {}",
                r.name,
                r.number,
                state_word(r.state),
                p.path
            )
        };
        egui::ComboBox::from_id_salt("ci-path-run")
            .selected_text(shown.map_or("Pick a run".to_owned(), |i| label(&runs.list[i])))
            .width(260.0)
            .show_ui(ui, |ui| {
                for (i, entry) in runs.list.iter().enumerate() {
                    if ui
                        .selectable_label(Some(i) == shown, label(entry))
                        .clicked()
                    {
                        acts.push(act(Act::PickRun(i)));
                    }
                }
            });
        let Some(explorer) = state.explorer.ready() else {
            return;
        };
        ui.label(RichText::new("against").color(t.text_dim));
        let picked = match (state.baseline, &explorer.compared) {
            (Baseline::None, _) => "nothing".to_owned(),
            (_, Some(c)) => format!("#{} ({})", c.run.info.number, short(&c.run.info.sha)),
            (_, None) => "no earlier successful run".to_owned(),
        };
        egui::ComboBox::from_id_salt("ci-path-baseline")
            .selected_text(picked)
            .width(220.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.baseline == Baseline::Latest, "Latest earlier success")
                    .clicked()
                {
                    acts.push(act(Act::PickBaseline(Baseline::Latest)));
                }
                if ui
                    .selectable_label(state.baseline == Baseline::None, "Nothing")
                    .clicked()
                {
                    acts.push(act(Act::PickBaseline(Baseline::None)));
                }
                for b in &explorer.baselines {
                    let text = format!(
                        "#{} · {} · {}",
                        b.number,
                        short(&b.sha),
                        b.finished
                            .zip(b.started)
                            .map_or("?".to_owned(), |(f, s)| ci::duration_label(f - s))
                    );
                    let on = state.baseline == Baseline::Run(b.id);
                    if ui.selectable_label(on, text).clicked() {
                        acts.push(act(Act::PickBaseline(Baseline::Run(b.id))));
                    }
                }
            });
        if ui
            .button("Open in CI view")
            .on_hover_text("The run's jobs, logs and actions")
            .clicked()
        {
            acts.push(Action::OpenCi {
                sha: explorer.run.info.sha.clone(),
                run: Some(explorer.run.info.id),
            });
        }
    });
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
