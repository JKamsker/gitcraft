//! The conflict editor's parts for a resolution git reused (rerere): the banner saying so, with
//! the way back to the conflicts, the reused text under each conflict, and the repository's
//! switch.

use egui::{RichText, Stroke, Ui};

use super::rerere::{RerereAct, Setup, forget_ops};
use super::{ConflictEditor, Reused};
use crate::repo::{Action, Dialog};
use crate::theme::{self, Tokens};

/// What the banner says for a file opened without its markers. `whole`: the result couldn't be
/// matched conflict by conflict, so it shows the whole file.
fn banner_text(reused: Reused, whole: bool) -> String {
    let check = crate::i18n::text("repo-core-0680");
    match (reused, whole) {
        (Reused::Rerere, false) => {
            crate::i18n::message("repo-core-0681", &[("check", check.to_string().into())])
        }
        (Reused::Rerere, true) => {
            crate::i18n::message("repo-core-0682", &[("check", check.to_string().into())])
        }
        (Reused::NoMarkers, false) => crate::i18n::text("repo-core-0683").to_owned(),
        (Reused::NoMarkers, true) => crate::i18n::text("repo-core-0684").to_owned(),
    }
}

/// The banner's way back to the conflicts, asked about first: what is in the file is lost.
fn back_action(path: &str, reused: Reused) -> (&'static str, &'static str, Action) {
    let lost = crate::i18n::text("repo-core-0685");
    match reused {
        Reused::Rerere => (
            crate::i18n::text("repo-core-0686"),
            crate::i18n::text("repo-core-0687"),
            Action::Dialog(Dialog::confirm(
                "Forget this resolution?",
                crate::i18n::message(
                    "repo-core-0689",
                    &[
                        ("path", path.to_string().into()),
                        ("lost", lost.to_string().into()),
                    ],
                ),
                crate::i18n::text("repo-core-0690"),
                forget_ops(path, true),
                None,
            )),
        ),
        Reused::NoMarkers => (
            crate::i18n::text("repo-core-0691"),
            crate::i18n::text("repo-core-0692"),
            Action::Dialog(Dialog::confirm(
                "Bring the conflicts back?",
                crate::i18n::message(
                    "repo-core-0694",
                    &[
                        ("path", path.to_string().into()),
                        ("lost", lost.to_string().into()),
                    ],
                ),
                crate::i18n::text("repo-core-0695"),
                forget_ops(path, false),
                None,
            )),
        ),
    }
}

impl ConflictEditor {
    /// Under the header, for a file opened without its markers: what was reused, and the way
    /// back to the conflicts.
    pub(super) fn reused_banner(
        &self,
        ui: &mut Ui,
        t: &Tokens,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let Some(reused) = self.reused else { return };
        egui::Frame::new()
            .fill(t.card)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    crate::icons::inline(ui, "history", 14.0, t.accent);
                    ui.label(RichText::new(banner_text(reused, self.manual)).color(t.text));
                    let (label, tip, action) = back_action(&self.path, reused);
                    if ui
                        .add_enabled(idle && !self.saving, egui::Button::new(label))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        acts.push(action);
                    }
                });
            });
    }
}

/// The text reused for a conflict, under its two sides; its border lit while it is the one
/// chosen.
pub(super) fn recorded_box(ui: &mut Ui, t: &Tokens, k: usize, text: &str, picked: bool) {
    let border = if picked { t.accent } else { t.field_border };
    egui::Frame::new()
        .fill(t.field)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(crate::i18n::text("repo-core-0696"))
                    .color(t.text_faint)
                    .small(),
            );
            let body = if text.is_empty() {
                crate::i18n::text("repo-core-0697")
            } else {
                text.trim_end_matches(['\r', '\n'])
            };
            egui::ScrollArea::horizontal()
                .id_salt(("conflict-recorded", k))
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(body).font(theme::mono(12.0))).extend());
                });
        });
}

/// The repository's switch for remembering resolutions (`setup`: how rerere is set up here,
/// `None` until read; `enabled`: whether it can be used now), and a warning when git stages
/// what it reuses by itself.
pub(super) fn rerere_switch(
    ui: &mut Ui,
    (setup, enabled): (Option<Setup>, bool),
    acts: &mut Vec<Action>,
) {
    let Some(setup) = setup else { return };
    let mut checked = setup.on;
    if ui
        .add_enabled(
            enabled,
            egui::Checkbox::new(&mut checked, crate::i18n::text("repo-core-0699")),
        )
        .on_hover_text(crate::i18n::text("repo-core-0700"))
        .changed()
    {
        acts.push(Action::Rerere(RerereAct::SetEnabled(checked)));
    }
    if setup.on && setup.stages {
        let t = Tokens::get(ui.ctx());
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(crate::i18n::text("repo-core-0701")),
            )
            .on_hover_text(crate::i18n::text("repo-core-0702"))
            .clicked()
        {
            acts.push(Action::Rerere(RerereAct::LeaveUnstaged));
        }
        ui.label(
            RichText::new(crate::i18n::text("repo-core-0703"))
                .color(t.warning)
                .small(),
        )
        .on_hover_text(crate::i18n::text("repo-core-0704"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_says_what_was_reused() {
        assert!(banner_text(Reused::Rerere, false).contains("as you did before"));
        assert!(banner_text(Reused::Rerere, true).contains("whole file"));
        assert!(banner_text(Reused::NoMarkers, false).contains("no conflict markers"));
        assert!(banner_text(Reused::NoMarkers, true).contains("whole file"));
    }

    #[test]
    fn forgetting_is_asked_about_and_only_forgets_what_rerere_reused() {
        let ops_of = |a: Action| match a {
            Action::Dialog(Dialog::Confirm { ops, .. }) => Some(ops),
            _ => None,
        };
        let forget = ops_of(back_action("a b", Reused::Rerere).2).unwrap_or_default();
        assert_eq!(forget.len(), 2);
        assert_eq!(forget[0][..2], ["rerere", "forget"]);
        let back = ops_of(back_action("a b", Reused::NoMarkers).2).unwrap_or_default();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0][..2], ["checkout", "-m"]);
    }
}
