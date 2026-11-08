//! PR review heatmap (#175 item 7): a pull request's files and hunks by what was reviewed, changed since, or has open threads.

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
    pub(super) fn review_heat_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn review_heat_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn review_heat_back(&mut self) -> bool {
        false
    }

    pub(super) fn review_heat_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
