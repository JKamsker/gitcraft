//! The buttons of a fold of unchanged lines: on a hunk's header for the lines above it, and on
//! the row after the last hunk for the lines below it.

use egui::Ui;

use super::context::{STEP, Unfold};
use super::paint::text_button;
use super::{DiffData, Kind};
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// Room between the controls and the header's text, and between two controls.
const GAP: f32 = 4.0;

/// The fold on row `i`, if it hides lines, laid out right to left from `right` (centred on
/// `y`) and never left of `left`, where the header's own text ends: what doesn't fit is left
/// out, the least needed first.
pub(super) fn controls(
    ui: &mut Ui,
    data: &DiffData,
    i: usize,
    (left, right, y): (f32, f32, f32),
    acts: &mut Vec<Action>,
) {
    let Some(ctx) = &data.context else { return };
    let Some(g) = ctx.gap_at(i) else { return };
    let gap = &ctx.gaps[g];
    let hidden = gap.hidden();
    let t = Tokens::get(ui.ctx());
    let step = hidden.min(STEP);
    let is_hunk = data.lines.get(i).is_some_and(|l| l.kind == Kind::Hunk);
    // Left to right, each with how much it is needed (higher stays longer).
    let mut items: Vec<(String, &str, Unfold, u8)> = Vec::new();
    if !gap.trailing {
        items.push((
            format!("↑ {step}"),
            crate::i18n::text("repo-core-1795"),
            Unfold::Up(step),
            4,
        ));
    }
    if gap.opens_down(g) {
        items.push((
            format!("↓ {step}"),
            crate::i18n::text("repo-core-1797"),
            Unfold::Down(step),
            4,
        ));
    }
    if hidden > STEP {
        items.push((
            crate::i18n::message("repo-core-1798", &[("hidden", (hidden).into())]),
            crate::i18n::text("repo-core-1799"),
            Unfold::All,
            2,
        ));
    }
    if is_hunk && ctx.function_opens(g) {
        items.push((
            crate::i18n::text("repo-core-1800").to_owned(),
            crate::i18n::text("repo-core-1801"),
            Unfold::Function,
            1,
        ));
    }
    let what = if hidden == 1 {
        crate::i18n::text("repo-core-1802").to_owned()
    } else {
        crate::i18n::message("repo-core-1803", &[("hidden", (hidden).into())])
    };
    let count = ui
        .painter()
        .layout_no_wrap(what, theme::medium(11.0), t.text_dim);
    let width = |label: &str| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), theme::medium(11.0), t.accent)
            .size()
            .x
            + 14.0
            + GAP
    };
    let room = right - left;
    let mut show_count = true;
    let needed = |items: &[(String, &str, Unfold, u8)], count_too: bool| {
        items.iter().map(|it| width(&it.0)).sum::<f32>()
            + if count_too { count.size().x + GAP } else { 0.0 }
    };
    // Drop the least needed until the rest fit; the count goes before the arrows.
    while needed(&items, show_count) > room {
        let least = items.iter().enumerate().min_by_key(|(_, it)| it.3);
        match least {
            Some((k, it)) if it.3 < 3 || !show_count => {
                items.remove(k);
            }
            _ if show_count => show_count = false,
            _ => break,
        }
    }
    let mut right = right;
    for (label, tip, how, _) in items.iter().rev() {
        let id = ui.id().with(("fold", i, label.as_str()));
        let (resp, at) = text_button(ui, right, y, label, t.accent, id);
        if resp.on_hover_text(*tip).clicked() {
            acts.push(Action::DiffUnfold { gap: g, how: *how });
        }
        right = at - GAP;
    }
    if show_count {
        let at = egui::pos2(right - count.size().x, y - count.size().y / 2.0);
        ui.painter().galley(at, count, t.text_dim);
    }
}
