//! The lane's mark by a commit: a small square (a round dot is CI's), filled green or red for
//! a result, outlined while it waits or runs, with what it means on hover; a click shows what
//! the test printed.

use egui::{CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, vec2};

use super::{DotState, LaneAct, dot_tip};
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// Half the square's side.
const HALF: f32 = 4.0;

/// How much room the lane takes at the end of the description.
pub(crate) const LANE_W: f32 = 14.0;

impl RepoTab {
    /// The lane's mark for the commit of tree `tree` around `center`, if it has one.
    pub(in crate::repo) fn test_dot(
        &self,
        ui: &Ui,
        id: Id,
        center: Pos2,
        tree: &str,
    ) -> Option<Action> {
        let state = self.test_lane.dot(tree)?;
        let t = Tokens::get(ui.ctx());
        let rect = Rect::from_center_size(center, vec2(HALF * 2.0 + 6.0, HALF * 2.0 + 6.0));
        let resp = ui.interact(rect, id, Sense::click());
        let half = if resp.hovered() { HALF + 1.0 } else { HALF };
        paint(ui, &t, center, half, state);
        let ran = self.test_lane.outputs.contains_key(tree);
        let tip = dot_tip(state, self.test_lane.command(), tree, ran);
        let clicked = resp.on_hover_text(tip).clicked();
        (clicked && ran).then(|| Action::TestLane(LaneAct::ShowOutput(tree.to_owned())))
    }
}

fn paint(ui: &Ui, t: &Tokens, center: Pos2, half: f32, state: DotState) {
    let square = Rect::from_center_size(center, vec2(half * 2.0, half * 2.0));
    let round = CornerRadius::same(2);
    let painter = ui.painter();
    match state {
        DotState::Good => {
            painter.rect_filled(square, round, t.success);
        }
        DotState::Bad => {
            painter.rect_filled(square, round, t.danger);
        }
        DotState::Failed => {
            painter.rect_stroke(
                square,
                round,
                Stroke::new(1.6, t.warning),
                StrokeKind::Inside,
            );
        }
        DotState::Queued => {
            painter.rect_stroke(
                square,
                round,
                Stroke::new(1.2, t.text_faint),
                StrokeKind::Inside,
            );
        }
        DotState::Running => {
            // Breathing while it runs.
            let time = ui.input(|i| i.time);
            let alpha = 0.45 + 0.55 * ((time * 3.0).sin() as f32 * 0.5 + 0.5);
            painter.rect_stroke(
                square,
                round,
                Stroke::new(1.6, t.accent.gamma_multiply(alpha)),
                StrokeKind::Inside,
            );
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

impl RepoTab {
    /// The lane's heading in the graph's header `header`: its icon, over the lane.
    pub(in crate::repo) fn test_lane_header(
        &self,
        ui: &Ui,
        header: Rect,
        cols: &crate::repo::columns::Cols,
    ) {
        let t = Tokens::get(ui.ctx());
        let center = egui::pos2(header.right() - cols.trailing() - 23.0, header.center().y);
        let rect = Rect::from_center_size(center, vec2(12.0, 12.0));
        crate::icons::paint(ui, rect, "flask-conical", 12.0, t.text_faint);
        let tip = match self.test_lane.command() {
            Some(command) => format!("Tests: `{command}` (test.default.command)"),
            None => {
                "Tests: no test command yet; Test commits… in a commit's menu sets one".to_owned()
            }
        };
        ui.interact(rect, ui.id().with("test-lane-header"), Sense::hover())
            .on_hover_text(tip);
    }
}
