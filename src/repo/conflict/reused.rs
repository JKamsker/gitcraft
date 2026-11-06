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
    let check = "Check it: nothing is staged until you click Save and mark resolved.";
    match (reused, whole) {
        (Reused::Rerere, false) => format!(
            "Resolved as you did before (git rerere). Each conflict below shows the text reused \
             for it. {check}"
        ),
        (Reused::Rerere, true) => format!(
            "Resolved as you did before (git rerere), but the result can't be matched conflict \
             by conflict, so it shows the whole file as it is now. {check}"
        ),
        (Reused::NoMarkers, false) => "This file has no conflict markers left, but git still \
                                       lists it as conflicted: a resolution was reused (git \
                                       rerere), or it was edited elsewhere. Each conflict \
                                       below shows the text now in its place."
            .to_owned(),
        (Reused::NoMarkers, true) => "This file has no conflict markers left, but git still \
                                      lists it as conflicted: a resolution was reused (git \
                                      rerere), or it was edited elsewhere. The result shows the \
                                      whole file as it is now."
            .to_owned(),
    }
}

/// The banner's way back to the conflicts, asked about first: what is in the file is lost.
fn back_action(path: &str, reused: Reused) -> (&'static str, &'static str, Action) {
    let lost = "What is in the file now is lost, saved or not.";
    match reused {
        Reused::Rerere => (
            "Forget this resolution",
            "git forgets how these conflicts were resolved before and puts the conflict \
             markers back in the file",
            Action::Dialog(Dialog::confirm(
                "Forget this resolution?",
                format!(
                    "git forgets how the conflicts of {path} were resolved before (git rerere \
                     forget), and puts their conflict markers back. {lost}"
                ),
                "Forget it",
                forget_ops(path, true),
                None,
            )),
        ),
        Reused::NoMarkers => (
            "Bring the conflicts back",
            "Puts the conflict markers back in the file (git checkout -m)",
            Action::Dialog(Dialog::confirm(
                "Bring the conflicts back?",
                format!("{path} gets its conflict markers back (git checkout -m). {lost}"),
                "Bring them back",
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
                RichText::new("Reused: the text now in place of this conflict")
                    .color(t.text_faint)
                    .small(),
            );
            let body = if text.is_empty() {
                "(nothing)"
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
            egui::Checkbox::new(&mut checked, "Remember resolutions"),
        )
        .on_hover_text(
            "git rerere, for this repository: how you resolve a conflict is recorded when you \
             commit or continue, and reused when the same conflict comes back. A reused \
             resolution shows here, and isn't staged until you save it. Turning it on also \
             sets rerere.autoUpdate to false here.",
        )
        .changed()
    {
        acts.push(Action::Rerere(RerereAct::SetEnabled(checked)));
    }
    if setup.on && setup.stages {
        let t = Tokens::get(ui.ctx());
        if ui
            .add_enabled(enabled, egui::Button::new("Leave them unstaged"))
            .on_hover_text("Sets rerere.autoUpdate to false for this repository")
            .clicked()
        {
            acts.push(Action::Rerere(RerereAct::LeaveUnstaged));
        }
        ui.label(
            RichText::new("git stages reused resolutions by itself here")
                .color(t.warning)
                .small(),
        )
        .on_hover_text(
            "rerere.autoUpdate is on: a file resolved as before is staged at once, so it never \
             shows here to be checked",
        );
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
