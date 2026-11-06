//! The conflict editor's parts for a resolution git reused (rerere): the banner saying so, with
//! the way to forget it, the reused text under each conflict, and the repository's switch.

use egui::{RichText, Stroke, Ui};

use super::rerere::RerereAct;
use super::{ConflictEditor, Reused};
use crate::repo::Action;
use crate::theme::{self, Tokens};

/// What the banner says for a file opened without its markers.
fn banner_text(reused: Reused, manual: bool) -> String {
    let mut text = match reused {
        Reused::Rerere => "git reused how you resolved these conflicts before (rerere). Check \
                           each one below: nothing is staged until you save."
            .to_owned(),
        Reused::NoMarkers => "No conflict markers are left in this file, though git still has \
                              it unmerged: a recorded resolution was reused (rerere), or it was \
                              edited elsewhere. Each conflict shows the text now in its place."
            .to_owned(),
    };
    if manual {
        text.push_str(
            " The text around the conflicts changed as well, so the result shows the file as it \
             is.",
        );
    }
    text
}

impl ConflictEditor {
    /// Under the header, for a file opened without its markers: what was reused, and the way
    /// back to the conflicts. `rerere`: whether it is on here.
    pub(super) fn reused_banner(
        &self,
        ui: &mut Ui,
        t: &Tokens,
        idle: bool,
        rerere: Option<bool>,
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
                    let (label, tip) = if rerere == Some(true) {
                        (
                            "Forget it and bring the conflicts back",
                            "git rerere forget: the recorded resolution is dropped, and the \
                             file gets its conflict markers again",
                        )
                    } else {
                        (
                            "Bring the conflicts back",
                            "The file gets its conflict markers again (git checkout -m)",
                        )
                    };
                    if ui
                        .add_enabled(idle && !self.saving, egui::Button::new(label))
                        .on_hover_text(tip)
                        .clicked()
                    {
                        acts.push(Action::Rerere(RerereAct::Forget(self.path.clone())));
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
                RichText::new("Reused resolution")
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

/// The repository's switch for remembering resolutions (`on`: whether rerere is on here,
/// `None` until read).
pub(super) fn rerere_switch(ui: &mut Ui, on: Option<bool>, acts: &mut Vec<Action>) {
    let Some(on) = on else { return };
    let mut checked = on;
    if ui
        .checkbox(&mut checked, "Remember resolutions")
        .on_hover_text(
            "git rerere, for this repository: how you resolve a conflict is recorded when you \
             commit or continue, and reused when the same conflict comes back. A reused \
             resolution is shown here and stays unstaged until you check it.",
        )
        .changed()
    {
        acts.push(Action::Rerere(RerereAct::SetEnabled(checked)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_says_what_was_reused() {
        assert!(banner_text(Reused::Rerere, false).contains("rerere"));
        assert!(banner_text(Reused::NoMarkers, false).contains("No conflict markers"));
        assert!(banner_text(Reused::Rerere, true).contains("as it is"));
    }
}
