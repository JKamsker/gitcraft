//! The dialog that asks before a destructive operation runs, and says what it will do.

use egui::{RichText, Ui};

use crate::repo::{Action, Refresh, Undo};
use crate::theme::{self, Tokens};

use super::preview::{self, WhatItDoes};
use super::{Outcome, cancel_button};

/// What a confirmation asks about.
pub(super) struct Asked<'a> {
    pub(super) title: &'a str,
    pub(super) body: &'a str,
    pub(super) button: &'a str,
    pub(super) ops: &'a [Vec<String>],
    /// An optional checkbox (label, ops to run instead when ticked), e.g. force delete.
    pub(super) alternative: &'a Option<(String, Vec<Vec<String>>)>,
    pub(super) refresh: Refresh,
    pub(super) undo: &'a Undo,
}

/// Asks before running the ops, or the `alternative` ones when its box is ticked, or before
/// taking the action `preview` has instead; shows what `preview` read.
pub(super) fn confirm(
    ui: &mut Ui,
    asked: Asked<'_>,
    use_alternative: &mut bool,
    preview: &mut WhatItDoes,
    safe: bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(asked.title).font(theme::semibold(15.0)));
    ui.add_space(6.0);
    ui.add(egui::Label::new(asked.body).wrap());
    preview::show(ui, preview);
    if let Some((label, _)) = asked.alternative {
        ui.add_space(6.0);
        ui.checkbox(use_alternative, label.as_str());
    }
    let steps = match (asked.alternative, *use_alternative) {
        (Some((_, alt)), true) => alt.as_slice(),
        _ => asked.ops,
    };
    let described = preview.instead.as_ref().map(|i| i.described.as_slice());
    preview::commands(ui, asked.title, steps, described);
    preview::undo_note(ui, asked.undo, preview);
    ui.add_space(14.0);
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let (fill, text) = if safe {
            (t.accent, t.accent_text)
        } else {
            (t.danger, egui::Color32::WHITE)
        };
        let danger = egui::Button::new(RichText::new(asked.button).color(text).font(theme::medium(12.5)))
            .fill(fill)
            .min_size(egui::vec2(90.0, 30.0));
        if ui.add(danger).clicked() {
            outcome = Outcome::Done(Box::new(match preview.instead.take() {
                Some(instead) => instead.action,
                None => ops_action(&asked, steps.to_vec()),
            }));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// Running `steps`, undoable as the dialog says.
fn ops_action(asked: &Asked<'_>, steps: Vec<Vec<String>>) -> Action {
    let label = asked.title.to_owned();
    let refresh = asked.refresh;
    match asked.undo {
        Undo::None => Action::Ops {
            label,
            ops: steps,
            refresh,
        },
        undo => Action::UndoableOps {
            label,
            ops: steps,
            refresh,
            undo: undo.clone(),
        },
    }
}
