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
    ui.label(
        RichText::new(crate::repo::messages::display_operation(asked.title))
            .font(theme::semibold(15.0)),
    );
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
        let danger = egui::Button::new(
            RichText::new(asked.button)
                .color(text)
                .font(theme::medium(12.5)),
        )
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

#[cfg(test)]
mod fluent_identity_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// Exercise the painted confirmation and its real click-to-action path without a repo,
    /// preview reader, worker, Git command or rewrite. The title remains DCO's classifier key.
    #[test]
    fn german_dco_confirmation_preserves_raw_operation_identity() {
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            for local_only in [false, true] {
                let branch = "feature/日本語 Commit";
                let raw = format!(
                    "Sign off the commits of {branch}{}",
                    if local_only { " (here only)" } else { "" }
                );
                let expected = if language == "de" {
                    format!(
                        "Commits von {branch} mit Signed-off-by versehen{}",
                        if local_only { " (nur hier)" } else { "" }
                    )
                } else {
                    raw.clone()
                };
                let title = raw.clone();
                let ops = vec![vec![
                    "rewrite-inert-placeholder".to_owned(),
                    "--literal=Commit".to_owned(),
                    branch.to_owned(),
                ]];
                let expected_ops = ops.clone();
                let mut preview = WhatItDoes::default();
                assert!(!preview.ask.reads_git());
                let fonts = std::cell::Cell::new(false);
                let mut h = Harness::new_ui_state(
                    move |ui, action: &mut Option<Action>| {
                        if !fonts.replace(true) {
                            crate::theme::install_fonts(ui.ctx());
                            return;
                        }
                        let outcome = confirm(
                            ui,
                            Asked {
                                title: &title,
                                body: "inert fixed-data confirmation",
                                button: "Confirm fixture",
                                ops: &ops,
                                alternative: &None,
                                refresh: Refresh::All,
                                undo: &Undo::None,
                            },
                            &mut false,
                            &mut preview,
                            false,
                        );
                        if let Outcome::Done(done) = outcome {
                            *action = Some(*done);
                        }
                        crate::a11y::end_frame(ui.ctx());
                    },
                    None,
                );
                h.run();
                h.get_by_label(&expected);
                h.get_by_role_and_label(egui::accesskit::Role::Button, "Confirm fixture")
                    .click();
                h.run();
                let Some(Action::Ops {
                    label,
                    ops,
                    refresh,
                }) = h.state()
                else {
                    panic!("confirmation must return its actual ops action")
                };
                assert_eq!(label, &raw);
                assert_eq!(ops, &expected_ops);
                assert_eq!(*refresh, Refresh::All);
            }
        }
        crate::i18n::set_language("en-US");
    }
}
