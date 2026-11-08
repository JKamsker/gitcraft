//! Worktree and agent constellation (#175 item 3): branches, worktrees, agent tasks, pull requests and builds as one navigable graph.

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
    pub(super) fn constellation_load(&mut self, _ctx: &egui::Context) {}

    pub(super) fn constellation_act(&mut self, act: Act, _ctx: &egui::Context) {
        match act {}
    }

    /// Esc: takes back what is selected, if anything (`false` closes the page).
    pub(super) fn constellation_back(&mut self) -> bool {
        false
    }

    pub(super) fn constellation_ui(&mut self, ui: &mut Ui, _acts: &mut Vec<Action>) {
        common::not_built(ui);
    }
}
