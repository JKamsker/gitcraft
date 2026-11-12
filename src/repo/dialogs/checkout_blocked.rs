//! A checkout git refused because files here are in the way: which they are, and how to check
//! out anyway.

use egui::{RichText, Ui};

use crate::repo::checkout_blocked::{BlockedCheckout, Choice};
use crate::repo::{Action, Refresh};
use crate::theme::Tokens;

use super::{Outcome, cancel_button, title};

/// How many files of each kind are listed by name.
const SHOWN: usize = 6;

pub(super) fn checkout_blocked(ui: &mut Ui, b: &BlockedCheckout) -> Outcome {
    ui.set_width(520.0);
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-checkout-blocked-title"),
        &crate::i18n::message(
            "repo-checkout-blocked-explanation",
            &[("label", b.label.as_str().into())],
        ),
        &t,
    );
    let lists = [
        ("repo-checkout-blocked-changed", &b.blockers.changed),
        ("repo-checkout-blocked-untracked", &b.blockers.untracked),
        ("repo-checkout-blocked-ignored", &b.blockers.ignored),
    ];
    for (heading, files) in lists.into_iter().filter(|(_, f)| !f.is_empty()) {
        ui.label(
            RichText::new(crate::i18n::message(
                heading,
                &[("count", files.len().into())],
            ))
            .color(t.text_dim)
            .small(),
        );
        for file in files.iter().take(SHOWN) {
            ui.add(egui::Label::new(RichText::new(file).monospace()).truncate());
        }
        if files.len() > SHOWN {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-core-1099",
                    &[("hidden", format!("{}", files.len() - SHOWN).into())],
                ))
                .color(t.text_faint)
                .small(),
            );
        }
        ui.add_space(6.0);
    }
    let mut outcome = Outcome::Open;
    for (i, choice) in b.blockers.choices().into_iter().enumerate() {
        ui.add_space(4.0);
        let (key, about) = keys(choice);
        let text = crate::i18n::text(key);
        let clicked = if i == 0 {
            crate::repo::widgets::primary_button(ui, text, true, 220.0).clicked()
        } else {
            let mut label = RichText::new(text);
            if choice == Choice::Force {
                label = label.color(t.danger);
            }
            ui.add(egui::Button::new(label).min_size(egui::vec2(220.0, 30.0)))
                .clicked()
        };
        ui.label(
            RichText::new(crate::i18n::text(about))
                .color(t.text_faint)
                .small(),
        );
        if clicked && choice == Choice::Carry {
            outcome = Outcome::Done(Box::new(Action::CarryCheckout(Box::new(b.clone()))));
        } else if clicked {
            outcome = Outcome::Done(Box::new(Action::UndoableOps {
                label: b.label.clone(),
                ops: b.retry(choice),
                refresh: Refresh::All,
                undo: b.undo.clone(),
            }));
        }
    }
    ui.add_space(10.0);
    if cancel_button(ui) {
        outcome = Outcome::Cancel;
    }
    outcome
}

/// The button's text and what it says it does.
fn keys(choice: Choice) -> (&'static str, &'static str) {
    match choice {
        Choice::OverwriteIgnored => (
            "repo-checkout-blocked-overwrite",
            "repo-checkout-blocked-overwrite-about",
        ),
        Choice::Carry => (
            "repo-checkout-blocked-carry",
            "repo-checkout-blocked-carry-about",
        ),
        Choice::Stash => (
            "repo-checkout-blocked-stash",
            "repo-checkout-blocked-stash-about",
        ),
        Choice::Force => (
            "repo-checkout-blocked-force",
            "repo-checkout-blocked-force-about",
        ),
    }
}
