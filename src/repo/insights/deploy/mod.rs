//! Deployment provenance flow (#175 item 9): the selected commit (or HEAD), its CI runs, the
//! artifacts they uploaded and the environments the forge lists, with which are known to
//! include the commit, how freshly that was reported, and what is queued or under way. A
//! node opens its run, artifact, deployment or environment, or selects the deployed commit.

mod flow;

#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod tests;

use egui::{Pos2, Ui};

use super::InsightsAct;
use super::common::{self, Slot, SlotState};
use super::graph::{self, Graph};
use crate::insights::deploy::{self as analysis, Flow};
use crate::repo::{Action, RepoTab};

/// What a node of the picture stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Commit,
    /// A run, by its index in the flow.
    Run(usize),
    /// An artifact: its run's index and its own.
    Artifact(usize, usize),
    /// An environment, by its index.
    Env(usize),
}

/// The picture of a flow, made once it is read.
pub(crate) struct Picture {
    graph: Graph,
    positions: Vec<Pos2>,
    targets: Vec<Target>,
    /// Artifacts left out of the picture (they are in the table).
    left_out: usize,
}

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {
    /// The commit the view is about: its name (`HEAD`, `selected commit`) and id.
    subject: Option<(String, String)>,
    /// What the view needs and doesn't have: a title, and what would make it available.
    unavailable: Option<(String, String)>,
    flow: Slot<Flow>,
    picture: Option<Picture>,
    selected: Option<Target>,
}

impl State {
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
    /// Pick a node (or a table row), or nothing.
    Select(Option<Target>),
}

fn act(act: Act) -> Action {
    Action::Insights(InsightsAct::Deploy(act))
}

impl RepoTab {
    /// Reads what the view shows (again).
    pub(super) fn deploy_load(&mut self, ctx: &egui::Context) {
        let subject = self.ci_path_subject();
        let available = analysis::availability(self.forge());
        let project = self.ci_project();
        let path = self.path.clone();
        let Some(state) = self.insights.as_mut().map(|v| &mut v.deploy) else {
            return;
        };
        state.subject = subject.clone();
        state.unavailable = None;
        let Some((name, sha)) = subject else {
            state.unavailable(
                "No commit",
                "Select a commit, or open a repository with one, to see where it is deployed."
                    .to_owned(),
            );
            return;
        };
        if let Err(why) = available {
            state.unavailable("No deployments to read", why);
            return;
        }
        let Some(project) = project else {
            state.unavailable(
                "No deployments to read",
                "The selected forge remote doesn't name a GitHub or GitLab project.".to_owned(),
            );
            return;
        };
        let request = state.flow.start();
        self.insight_forge_then(
            ctx,
            move || analysis::read(&path, &project, &sha, (&name, &sha)),
            move |tab, result, ctx| tab.deploy_read(request, result, ctx),
        );
    }

    fn deploy_read(&mut self, request: u64, result: Result<Flow, String>, ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.deploy) else {
            return;
        };
        if !state.flow.accept(request, result) {
            return;
        }
        state.unavailable = state
            .flow
            .error()
            .filter(|error| super::ci_path::tool_problem(error))
            .map(|error| ("The forge's tool is needed".to_owned(), error.to_owned()));
        if state.unavailable.is_some() {
            return;
        }
        state.picture = state.flow.ready().map(|f| flow::picture(f, ctx));
        let valid = state
            .picture
            .as_ref()
            .is_some_and(|p| state.selected.is_none_or(|s| p.targets.contains(&s)));
        if !valid {
            state.selected = None;
        }
    }

    pub(super) fn deploy_act(&mut self, act: Act, _ctx: &egui::Context) {
        let Some(state) = self.insights.as_mut().map(|v| &mut v.deploy) else {
            return;
        };
        match act {
            Act::Select(target) => state.selected = target,
        }
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn deploy_back(&mut self) -> bool {
        self.insights
            .as_mut()
            .and_then(|v| v.deploy.selected.take())
            .is_some()
    }

    pub(super) fn deploy_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let subject_now = self.ci_path_subject();
        let Some(view) = self.insights.as_deref() else {
            return;
        };
        let (state, table) = (&view.deploy, view.table);
        if let Some((title, why)) = &state.unavailable {
            common::unavailable(ui, title, why);
            return;
        }
        match common::slot_state(ui, &state.flow, "the deployments") {
            SlotState::Ready => {}
            SlotState::Retry => {
                acts.push(Action::Insights(InsightsAct::Reload));
                return;
            }
            SlotState::Waiting => return,
        }
        let Some(flow) = state.flow.ready() else {
            return;
        };
        let mut provenance = flow.provenance.clone();
        provenance.check_tips(|name| match &subject_now {
            Some((now, sha)) if now == name => Some(sha.clone()),
            _ => None,
        });
        if provenance.stale.is_some() {
            provenance.stale = Some("The commit the view is about changed".to_owned());
        }
        common::provenance(ui, &provenance);
        flow::legend(ui);
        flow::caution(ui, flow);
        egui::ScrollArea::vertical()
            .id_salt("deploy-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !table && let Some(picture) = &state.picture {
                    let selected = state
                        .selected
                        .and_then(|s| picture.targets.iter().position(|t| *t == s))
                        .map(graph::Pick::Node);
                    let height = (picture.graph.nodes.len() as f32 * 28.0).clamp(220.0, 420.0);
                    ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                        let picked = graph::show(
                            ui,
                            "deploy-flow",
                            &picture.graph,
                            &picture.positions,
                            selected,
                            height,
                        );
                        if let Some(graph::Pick::Node(i)) = picked.clicked
                            && let Some(target) = picture.targets.get(i)
                        {
                            let pick = (state.selected != Some(*target)).then_some(*target);
                            acts.push(act(Act::Select(pick)));
                        }
                    });
                    if picture.left_out > 0 {
                        common::empty(
                            ui,
                            &format!(
                                "{} more artifacts are in the table, not in the picture.",
                                picture.left_out
                            ),
                        );
                    }
                }
                flow::environments(ui, flow, state.selected, acts);
                if table {
                    flow::runs(ui, flow, state.selected, acts);
                }
                flow::detail(ui, flow, state.selected, acts);
            });
    }
}
