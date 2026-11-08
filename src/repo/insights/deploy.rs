//! Deployment provenance flow (#175 item 9): commit to run to artifact to environment, as far as the forge records it.

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
    pub(super) fn deploy_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn deploy_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn deploy_back(&mut self) -> bool {
        false
    }

    pub(super) fn deploy_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
