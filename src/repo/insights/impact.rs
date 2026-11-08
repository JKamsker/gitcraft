//! Change impact graph (#175 item 1): what refers to the symbols a change touched, at the base and head revisions compared.

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
    pub(super) fn impact_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn impact_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn impact_back(&mut self) -> bool {
        false
    }

    pub(super) fn impact_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
