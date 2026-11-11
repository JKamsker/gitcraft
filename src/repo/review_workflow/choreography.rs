//! Cross-repository release planning and compatibility rehearsal. No publishing operations.
use super::{
    contracts::{self, Observation},
    store,
    tools::ToolsAct,
};
use crate::{
    git,
    repo::{Action, RepoTab},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize, Default)]
struct Component {
    name: String,
    repo: String,
    revisions: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct Rule {
    dependent: String,
    at: usize,
    prerequisite: String,
    min: usize,
    reason: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub(crate) struct Plan {
    components: Vec<Component>,
    rules: Vec<Rule>,
    command: String,
    trusted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Stage {
    label: String,
    commit: String,
    tree: String,
    // Missing provenance in older evidence must never reinterpret an explicit user label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    generated_index: Option<usize>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Source {
    name: String,
    repo: PathBuf,
    stages: Vec<Stage>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Combination {
    stages: Vec<usize>,
    allowed: bool,
    observed: Option<Observation>,
    error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Report {
    sources: Vec<Source>,
    rules: Vec<Rule>,
    command: String,
    states: Vec<Combination>,
    route: Option<Vec<Vec<usize>>>,
    tested: bool,
    stopped: bool,
    cleanup: Vec<String>,
}
#[derive(Default)]
pub(crate) struct State {
    plan: Plan,
    busy: bool,
    stop: git::RunStop,
    report: Option<Report>,
    error: Option<String>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Save(Plan),
    Load,
    Plan(Plan),
    Rehearse(Plan),
    Stop,
}
impl RepoTab {
    pub(in crate::repo) fn choreography_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.choreography.stop.stop();
            return;
        }
        if self.review_tools.choreography.busy {
            return;
        }
        self.review_tools.choreography.busy = true;
        self.review_tools.choreography.stop = git::RunStop::default();
        let (repo, stop) = (
            self.path.clone(),
            self.review_tools.choreography.stop.clone(),
        );
        self.then_or(
            ctx,
            true,
            Err,
            move || match act {
                Act::Save(plan) => {
                    store::save(&repo, "release-board", "plan", &plan)?;
                    Ok((Some(plan), None))
                }
                Act::Load => Ok((
                    store::load::<Plan>(&repo, "release-board", "plan")?,
                    store::load::<Report>(&repo, "release-rehearsal-evidence", "latest")?,
                )),
                Act::Plan(plan) => run(&repo, &plan, false, &stop).map(|r| (None, Some(r))),
                Act::Rehearse(plan) => run(&repo, &plan, true, &stop).map(|r| (None, Some(r))),
                Act::Stop => Ok((None, None)),
            },
            |tab, result, _| {
                let s = &mut tab.review_tools.choreography;
                s.busy = false;
                match result {
                    Ok((plan, report)) => {
                        s.error = None;
                        if let Some(plan) = plan {
                            s.plan = plan;
                        }
                        if report.is_some() {
                            s.report = report;
                        }
                    }
                    Err(e) => s.error = Some(e),
                }
            },
        );
    }
}
#[cfg(test)]
#[path = "choreography/fluent_review_r1_tests.rs"]
mod fluent_review_r1_tests;
#[cfg(test)]
#[path = "choreography/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "choreography/tests.rs"]
mod tests;

mod planning;
use planning::{combinations, display_stage, resolve, route};
#[cfg(test)]
use planning::{stage_input, valid_name};

mod rehearsal;
use rehearsal::run;

mod presentation;
pub(super) use presentation::show;
