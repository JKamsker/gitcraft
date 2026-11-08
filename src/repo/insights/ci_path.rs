//! CI critical path and regression explorer (#175 item 2): a run's jobs and steps on a time line, the critical path, waiting time, and what got slower than an earlier run.

use egui::Ui;

use super::common;
use crate::repo::{Action, RepoTab};

/// What the view keeps.
#[derive(Default)]
pub(crate) struct State {}

/// What the view asks for.
#[derive(Clone, Debug)]
pub(crate) enum Act {}

impl RepoTab {
    /// Reads what the view shows (again).
    pub(super) fn ci_path_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn ci_path_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn ci_path_back(&mut self) -> bool {
        false
    }

    pub(super) fn ci_path_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
