//! Backport rehearsals never move a user's branch: patches and trusted regression commands
//! run in owned detached scratch worktrees. Patch application and behavior stay separate.
use super::{store, tools::ToolsAct};
use crate::{
    git,
    repo::{
        Action, RepoTab,
        drag_drop::{DragItem, Payload},
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Observation {
    pub commit: String,
    pub tree: String,
    pub command: String,
    pub code: Option<i32>,
    pub stopped: bool,
    pub lines: Vec<String>,
    pub machine: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Row {
    target: String,
    tip: String,
    original: String,
    source: String,
    applied: Option<String>,
    error: Option<String>,
    test: Option<Observation>,
    adaptation: Option<(String, String)>,
    cleanup: Option<String>,
}
#[derive(Clone, Default)]
pub(crate) struct Plan {
    fix: String,
    targets: String,
    command: String,
    trusted: bool,
}
#[derive(Default)]
pub(crate) struct State {
    plan: Plan,
    busy: bool,
    stop: git::RunStop,
    rows: Option<Result<Vec<Row>, String>>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.stop.stop();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Run(Plan),
    Stop,
    Load,
}
impl RepoTab {
    pub(in crate::repo) fn contract_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.contracts.stop.stop();
            return;
        }
        if self.review_tools.contracts.busy {
            return;
        }
        self.review_tools.contracts.busy = true;
        self.review_tools.contracts.stop = git::RunStop::default();
        let (repo, stop) = (self.path.clone(), self.review_tools.contracts.stop.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || match act {
                Act::Run(plan) => run(&repo, &plan, &stop),
                Act::Load => Ok(
                    store::load::<Vec<Row>>(&repo, "backport-contracts", "latest")?
                        .unwrap_or_default(),
                ),
                Act::Stop => Ok(vec![]),
            },
            |tab, result, _| {
                tab.review_tools.contracts.busy = false;
                tab.review_tools.contracts.rows = Some(result);
            },
        );
    }
}
#[cfg(test)]
#[path = "contracts/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "contracts/tests.rs"]
mod tests;

mod execution;
use execution::run;
#[cfg(test)]
use execution::targets;
#[cfg(test)]
use execution::{evidence_roots, retain_evidence};
pub(super) use execution::{observe, resolve};

mod presentation;
#[cfg(test)]
use presentation::observation_summary;
pub(super) use presentation::{show, show_observation};
