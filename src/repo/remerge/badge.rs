//! The badge of a merge row in the graph that resolved conflicts or edited beyond the merge:
//! a click shows what it resolved.

use egui::{Align2, CornerRadius, Painter, Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::git::{Commit, Remerge};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// What a merge row's badge says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) struct RowBadge {
    /// Files whose conflicts were resolved.
    pub(in crate::repo) conflicts: usize,
    /// Files edited beyond the merge.
    pub(in crate::repo) edits: usize,
}

impl RowBadge {
    /// The badge of a merge that resolved `remerge`; none for a clean one.
    pub(in crate::repo) fn of(remerge: &Remerge) -> Option<Self> {
        let badge = RowBadge {
            conflicts: remerge.conflicted().count(),
            edits: remerge.edited().count(),
        };
        (badge.conflicts + badge.edits > 0).then_some(badge)
    }

    /// The word on it.
    pub(in crate::repo) fn label(self) -> &'static str {
        if self.conflicts > 0 {
            "resolved"
        } else {
            "edited"
        }
    }

    /// What hovering it says.
    pub(in crate::repo) fn tooltip(self) -> String {
        let files = |n: usize| format!("{n} file{}", if n == 1 { "" } else { "s" });
        let mut parts = Vec::new();
        if self.conflicts > 0 {
            parts.push(format!("conflicts resolved in {}", files(self.conflicts)));
        }
        if self.edits > 0 {
            parts.push(format!("{} edited beyond the merge", files(self.edits)));
        }
        format!(
            "This merge differs from the one git makes: {}.\nClick to see what it resolved.",
            parts.join(", ")
        )
    }

    /// Its width as drawn.
    pub(in crate::repo) fn width(self, painter: &Painter) -> f32 {
        let galley = painter.layout_no_wrap(self.label().to_owned(), font(), egui::Color32::WHITE);
        galley.size().x + 12.0
    }
}

fn font() -> egui::FontId {
    theme::medium(10.5)
}

impl RepoTab {
    /// The badge of graph row `commit`, when it is a merge read to have resolved something.
    pub(in crate::repo) fn remerge_badge(&self, commit: &Commit) -> Option<RowBadge> {
        if commit.parents.len() != 2 {
            return None;
        }
        RowBadge::of(self.remerges.of(&commit.hash)?.as_ref().ok()?)
    }

    /// Draws `badge` of row `i` (commit `hash`) ending at `right_center`; a click selects the
    /// row and shows what its merge resolved.
    pub(in crate::repo) fn show_remerge_badge(
        &self,
        ui: &Ui,
        painter: &Painter,
        (i, hash): (usize, &str),
        badge: RowBadge,
        right_center: Pos2,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let w = badge.width(painter);
        let rect = Rect::from_min_size(
            pos2(right_center.x - w, right_center.y - 8.0),
            vec2(w, 16.0),
        );
        let resp = ui
            .interact(rect, ui.id().with(("remerge-badge", hash)), Sense::click())
            .on_hover_text(badge.tooltip());
        // Read out by screen readers (it is only painted).
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, badge.label())
        });
        let color = t.warning;
        let fill = if resp.hovered() { 0.30 } else { 0.18 };
        painter.rect_filled(rect, CornerRadius::same(8), color.gamma_multiply(fill));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            badge.label(),
            font(),
            color,
        );
        if resp.clicked() {
            acts.push(Action::Select(i));
            acts.push(Action::ShowRemerge(true));
        }
    }
}
