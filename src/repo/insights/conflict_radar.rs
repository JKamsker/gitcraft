//! Branch conflict radar (#175 item 6): active branches and worktrees joined by the files they both change and the conflicts a dry-run merge finds.

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
    pub(super) fn conflict_radar_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn conflict_radar_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn conflict_radar_back(&mut self) -> bool {
        false
    }

    pub(super) fn conflict_radar_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
