//! Above the changes of a Unity project: assets staged without their `.meta` (or the other way
//! round), and the hint that its set-up for git needs fixing.

use egui::{RichText, Ui};

use super::UnityAct;
use crate::git::{MetaKind, MetaProblem};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// One `.meta` problem in words.
pub(super) fn describe(p: &MetaProblem) -> String {
    match p.kind {
        MetaKind::AddedWithoutMeta => format!("{} is staged without {}", p.path, p.partner),
        MetaKind::DeletedWithoutMeta => {
            format!("{} is deleted but {} stays", p.path, p.partner)
        }
        MetaKind::MetaWithoutAsset => format!("{} is staged without {}", p.path, p.partner),
        MetaKind::MetaDeletedAssetKept => {
            format!("{} is deleted but {} stays", p.path, p.partner)
        }
    }
}

impl RepoTab {
    pub(in crate::repo) fn unity_banners(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let state = &self.unity;
        if state.project.is_none() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        if !state.meta.is_empty() {
            let text = match state.meta.as_slice() {
                [one] => format!(
                    "Unity: {}. Committed so, its references break for everyone else.",
                    describe(one)
                ),
                many => format!(
                    "Unity: {} staged assets and .meta files are missing their other half \
                     (first: {}). Committed so, their references break for everyone else.",
                    many.len(),
                    describe(&many[0])
                ),
            };
            let stage = self.stage_partners_action();
            let idle = self.busy_op.is_none();
            strip(ui, &t, t.warning, &text, |ui| {
                if let Some(stage) = stage
                    && ui
                        .add_enabled(idle, egui::Button::new("Stage the other halves"))
                        .clicked()
                {
                    acts.push(stage);
                }
            });
        }
        let needs_setup = matches!(&state.setup, Some(Ok(s)) if !s.all_good());
        if needs_setup && !state.hint_dismissed {
            let text = "This Unity project isn't fully set up for git.";
            strip(ui, &t, t.accent, text, |ui| {
                if ui.button("Dismiss").clicked() {
                    acts.push(Action::Unity(UnityAct::DismissHint));
                }
                if ui.button("Unity set-up…").clicked() {
                    acts.push(Action::Unity(UnityAct::OpenPanel));
                }
            });
        }
    }
}

/// A tinted strip with the Unity icon and `text`, and `buttons` below it on the right.
fn strip(ui: &mut Ui, t: &Tokens, color: egui::Color32, text: &str, buttons: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.14))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::inline(ui, "gamepad-2", 14.0, color);
                ui.add(egui::Label::new(RichText::new(text).color(t.text).small()).wrap());
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), buttons);
            });
        });
}
