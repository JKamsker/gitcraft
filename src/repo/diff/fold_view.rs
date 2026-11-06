//! The buttons of a fold of unchanged lines: on a hunk's header for the lines above it, and on
//! the row after the last hunk for the lines below it.

use egui::Ui;

use super::context::{STEP, Unfold};
use super::paint::text_button;
use super::{DiffData, Kind};
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// The fold on row `i`, if it hides lines, laid out right to left from `right`, centred on `y`.
pub(super) fn controls(
    ui: &mut Ui,
    data: &DiffData,
    i: usize,
    (right, y): (f32, f32),
    acts: &mut Vec<Action>,
) {
    let Some(ctx) = &data.context else { return };
    let Some(g) = ctx.gap_at(i) else { return };
    let gap = &ctx.gaps[g];
    let hidden = gap.hidden();
    let t = Tokens::get(ui.ctx());
    let mut right = right;
    let mut button = |ui: &mut Ui, label: &str, tip: &str, how: Unfold| {
        let id = ui.id().with(("fold", i, label));
        let (resp, left) = text_button(ui, right, y, label, t.accent, id);
        if resp.on_hover_text(tip).clicked() {
            acts.push(Action::DiffUnfold { gap: g, how });
        }
        right = left - 4.0;
    };
    let is_hunk = data.lines.get(i).is_some_and(|l| l.kind == Kind::Hunk);
    if is_hunk && has_function(data, i) {
        button(
            ui,
            "Show the enclosing function",
            "Show the unchanged lines of the function this hunk is in, above and below it",
            Unfold::Function,
        );
    }
    if hidden > STEP {
        button(
            ui,
            &format!("Show all {hidden}"),
            "Show every unchanged line folded here",
            Unfold::All,
        );
    }
    let step = hidden.min(STEP);
    if gap.opens_down(g) {
        button(
            ui,
            &format!("↓ {step}"),
            "Show the unchanged lines right below the hunk above",
            Unfold::Down(step),
        );
    }
    if !gap.trailing {
        button(
            ui,
            &format!("↑ {step}"),
            "Show the unchanged lines right above this hunk",
            Unfold::Up(step),
        );
    }
    let what = if hidden == 1 {
        "1 unchanged line".to_owned()
    } else {
        format!("{hidden} unchanged lines")
    };
    let galley = ui
        .painter()
        .layout_no_wrap(what, theme::medium(11.0), t.text_dim);
    let at = egui::pos2(right - galley.size().x - 4.0, y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, t.text_dim);
}

/// Whether git named a function for the hunk whose header is row `i`.
fn has_function(data: &DiffData, i: usize) -> bool {
    data.lines
        .get(i)
        .and_then(|l| super::context::header_numbers(&l.raw))
        .is_some_and(|(.., tail)| !tail.trim().is_empty())
}
