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
        self.diff_can_split()
            && (Settings::get(ctx).diff_beside_graph
                || self
                    .diff
                    .as_ref()
                    .is_some_and(|view| self.commit_changes.graph_with_diff(&view.key)))
    }

    /// The graph on top, the diff below, the border between them dragged to resize (egui keeps
    /// the height for the session). Without room for both, the diff in the graph's place.
    pub(super) fn graph_diff_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let height = ui.available_height();
        if height < 2.0 * MIN_HEIGHT {
            self.diff_view(ui, acts);
            return;
        }
        // Just opened below the graph: the selected row may be under the diff now.
        let frame = ui.ctx().cumulative_frame_nr();
        let seen = egui::Id::new(("diff_below_graph_frame", &self.path));
        let prev = ui.data(|d| d.get_temp::<u64>(seen));
        ui.data_mut(|d| d.insert_temp(seen, frame));
        if prev.is_none_or(|p| frame > p + 1) {
            self.scroll_to = self.scroll_to.or(self.selected);
        }
        let most = height - MIN_HEIGHT;
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
    /// the diffs that can show there (whether it showed).
    pub(super) fn split_toggle(&self, ui: &mut Ui) -> bool {
        if !self.diff_can_split() {
            return false;
        }
        let t = Tokens::get(ui.ctx());
        let mut settings = Settings::get(ui.ctx());
        let on = settings.diff_beside_graph;
        let tip = if on {
            crate::i18n::text("repo-core-2414")
        } else {
            crate::i18n::text("repo-core-2415")
        };
        let button = icons::small_button(ui, "panel-bottom", tip);
        if on {
            widgets::active_underline(ui, button.rect, t.accent);
        }
        if button.clicked() {
            settings.diff_beside_graph = !on;
            settings.set(ui.ctx());
        }
        true
    }

    /// Leaves a pull request's or a commit's builds page (or the insights) for the graph: a
    /// diff shown over it was one of its files, and goes with it rather than staying below the
    /// graph.
    pub(super) fn leave_pages(&mut self) {
        self.file_comparison = None;
        self.tour = None;
        self.versions.view = None;
        self.insights = None;
        if (self.pr_view.is_some() || self.ci_view.is_some()) && self.conflict.is_none() {
            self.diff = None;
        }
        self.close_pr();
        self.ci_view = None;
    }
}
