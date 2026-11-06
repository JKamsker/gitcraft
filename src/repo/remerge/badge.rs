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

    /// The words on it, the full ones first, then the short ones for a narrow row.
    fn labels(self) -> [&'static str; 2] {
        match (self.conflicts > 0, self.edits > 0) {
            (true, true) => ["resolved + edited", "resolved +"],
            (true, false) => ["resolved"; 2],
            _ => ["edited"; 2],
        }
    }

    /// The words on it that fit in `room` beside some of the commit's summary, and their
    /// width; `None` when none do.
    pub(in crate::repo) fn fit(self, painter: &Painter, room: f32) -> Option<(&'static str, f32)> {
        self.labels()
            .into_iter()
            .map(|label| (label, width(painter, label)))
            .find(|(_, w)| room > w + MIN_SUMMARY)
    }

    /// What hovering it says.
    pub(in crate::repo) fn tooltip(self) -> String {
        let files = |n: usize| format!("{n} file{}", if n == 1 { "" } else { "s" });
        let mut parts = Vec::new();
        if self.conflicts > 0 {
            parts.push(format!("conflicts resolved in {}", files(self.conflicts)));
        }
        if self.edits > 0 {
            parts.push(format!(
                "{} changed beyond that (by hand, or a merge made with other options)",
                files(self.edits)
            ));
        }
        format!(
            "This merge differs from the one git makes of its parents: {}.\nClick to see what \
             it resolved.",
            parts.join("; ")
        )
    }
}

/// What is left at least of a commit's summary beside its badge.
const MIN_SUMMARY: f32 = 50.0;

/// The width of a badge saying `label`.
fn width(painter: &Painter, label: &str) -> f32 {
    let galley = painter.layout_no_wrap(label.to_owned(), font(), egui::Color32::WHITE);
    galley.size().x + 12.0
}

fn font() -> egui::FontId {
    theme::medium(11.0)
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
        (badge, label, w): (RowBadge, &str, f32),
        right_center: Pos2,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let rect = Rect::from_min_size(
            pos2(right_center.x - w, right_center.y - 9.0),
            vec2(w, 18.0),
        );
        let resp = ui
            .interact(rect, ui.id().with(("remerge-badge", hash)), Sense::click())
            .on_hover_text(badge.tooltip());
        // Read out by screen readers (it is only painted).
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, badge.tooltip())
        });
        // Not the warning colour: search matches are marked with that.
        let color = t.accent;
        let fill = if resp.hovered() { 0.30 } else { 0.18 };
        painter.rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(fill));
        painter.text(rect.center(), Align2::CENTER_CENTER, label, font(), color);
        if resp.clicked() {
            acts.push(Action::Select(i));
            acts.push(Action::ShowRemerge(true));
        }
    }
}
