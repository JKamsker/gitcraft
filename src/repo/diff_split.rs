//! The diff below the graph instead of in its place (a setting, from the diff's header): the
//! graph stays on top and keeps working, the diff follows what is opened, and the border
//! between them is dragged to share the height.

use egui::Ui;

use super::{Action, DiffKey, RepoTab, widgets};
use crate::git;
use crate::icons;
use crate::settings::Settings;
use crate::theme::Tokens;

/// The least height each of the two keeps.
const MIN_HEIGHT: f32 = 120.0;

impl RepoTab {
    /// The diff open is one that can show below the graph: a commit's, the working tree's or
    /// between two commits, over nothing but the graph. The conflict and rebase editors, the
    /// diffs of a file's history, of a pull request or over the builds keep their own place.
    pub(super) fn diff_can_split(&self) -> bool {
        let Some(diff) = &self.diff else {
            return false;
        };
        let conflicted = matches!(&diff.key, DiffKey::Work(f) if f.area == git::Area::Conflicted);
        !conflicted
            && self.conflict.is_none()
            && self.rebase.is_none()
            && self.file_view.is_none()
            && self.ci_view.is_none()
            && self.pr_view.is_none()
    }

    /// The diff shows below the graph now.
    pub(super) fn diff_beside_graph(&self, ctx: &egui::Context) -> bool {
        Settings::get(ctx).diff_beside_graph && self.diff_can_split()
    }

    /// The graph on top, the diff below, the border between them dragged to resize (egui keeps
    /// the height for the session).
    pub(super) fn split_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let height = ui.available_height();
        let most = (height - MIN_HEIGHT).max(MIN_HEIGHT);
        egui::Panel::bottom("repo_diff_below_graph")
            .resizable(true)
            .default_size(height * 0.55)
            .size_range(MIN_HEIGHT..=most)
            .frame(
                egui::Frame::new()
                    .fill(t.dock)
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| self.diff_view(ui, acts));
        self.graph_view(ui, acts);
    }

    /// The header's button putting the diff below the graph, or back in its place. Only for
    /// the diffs that can show there.
    pub(super) fn split_toggle(&self, ui: &mut Ui) {
        if !self.diff_can_split() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let mut settings = Settings::get(ui.ctx());
        let on = settings.diff_beside_graph;
        let tip = if on {
            "Show the diff in place of the graph"
        } else {
            "Show the diff below the graph (the graph stays usable; drag the border to resize)"
        };
        let button = icons::small_button(ui, "panel-bottom", tip);
        if on {
            widgets::active_underline(ui, button.rect, t.accent);
        }
        if button.clicked() {
            settings.diff_beside_graph = !on;
            settings.set(ui.ctx());
        }
    }
}
