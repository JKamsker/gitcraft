//! The lane's mark by a commit: a small square (a round dot is CI's), green with a tick or red
//! with a cross for a result, outlined while it waits or runs, amber with a dot when it couldn't
//! be tested, with what it means on hover; a click shows what the test printed.

use egui::{CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

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
        crate::a11y::a11y(&resp, egui::WidgetType::Button, &tip);
        let clicked = resp.on_hover_text(tip).clicked();
        (clicked && ran).then(|| Action::TestLane(LaneAct::ShowOutput(tree.to_owned())))
    }
}

fn paint(ui: &Ui, t: &Tokens, center: Pos2, half: f32, state: DotState) {
    let square = Rect::from_center_size(center, vec2(half * 2.0, half * 2.0));
    let round = CornerRadius::same(2);
    let painter = ui.painter();
    match state {
        // A tick or a cross in it too: not by colour alone.
        DotState::Good => {
            painter.rect_filled(square, round, t.success);
            let mark = Stroke::new(1.4, t.card);
            let (l, r) = (square.left() + half * 0.45, square.right() - half * 0.4);
            let bottom = pos2(center.x - half * 0.15, square.bottom() - half * 0.5);
            painter.line_segment([pos2(l, center.y), bottom], mark);
            painter.line_segment([bottom, pos2(r, square.top() + half * 0.5)], mark);
        }
        DotState::Bad => {
            painter.rect_filled(square, round, t.danger);
            let mark = Stroke::new(1.4, t.card);
            let inner = square.shrink(half * 0.55);
            painter.line_segment([inner.left_top(), inner.right_bottom()], mark);
            painter.line_segment([inner.right_top(), inner.left_bottom()], mark);
        }
        // Outlined, with a dot in it: it couldn't be tested.
        DotState::Failed => {
            painter.rect_stroke(
                square,
                round,
                Stroke::new(1.6, t.warning),
                StrokeKind::Inside,
            );
            painter.circle_filled(center, 1.3, t.warning);
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
    /// The lane's heading in the graph's header `header`: its icon, over the lane; a click
    /// opens Test commits….
    pub(in crate::repo) fn test_lane_header(
        &self,
        ui: &Ui,
        header: Rect,
        cols: &crate::repo::columns::Cols,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        let center = pos2(header.right() - cols.trailing() - 23.0, header.center().y);
        let rect = Rect::from_center_size(center, vec2(12.0, 12.0));
        crate::icons::paint(ui, rect, "flask-conical", 12.0, t.text_faint);
        let tip = match self.test_lane.command() {
            Some(command) => crate::i18n::message(
                "repo-workflows-test_lane-test-lane-header-9b3f6adb",
                &[("command", (command).clone().into())],
            ),
            None => {
                crate::i18n::text("repo-workflows-test_lane-test-lane-header-f4c5a803").to_owned()
            }
        };
        let id = ui.id().with("test-lane-header");
        let resp = ui.interact(rect.expand(3.0), id, Sense::click());
        crate::a11y::a11y(&resp, egui::WidgetType::Button, &tip);
        if resp.on_hover_text(tip).clicked() && !self.test_lane.running() {
            acts.push(self.test_dialog(None));
        }
    }
}
