//! Code complexity and churn timeline (#175 item 12): size, churn and complexity of files and folders over the commits read, and rising hotspots.

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
    pub(super) fn churn_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn churn_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn churn_back(&mut self) -> bool {
        false
    }

    pub(super) fn churn_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
