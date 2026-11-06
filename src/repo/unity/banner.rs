//! Above the changes of a Unity project: assets staged without their `.meta` (or the other way
//! round), and the hint that its set-up for git needs fixing.

use egui::{RichText, Ui};

use super::UnityAct;
use crate::git::{MetaKind, MetaProblem};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// How many problems the banner's tooltip lists.
const LISTED: usize = 15;

/// One `.meta` problem in words, with what follows from it.
pub(super) fn describe(p: &MetaProblem) -> String {
    match p.kind {
        MetaKind::AddedWithoutMeta => format!(
            "{} has no {} staged: others get a new GUID for it and its references break.",
            p.path, p.partner
        ),
        MetaKind::DeletedWithoutMeta => format!(
            "{} is deleted but {} stays: Unity will warn about the leftover.",
            p.path, p.partner
        ),
        MetaKind::MetaWithoutAsset => format!(
            "{} is staged without {}: if that is an empty folder, git keeps no empty folders \
             (add a file to it, or leave its .meta out); else Unity deletes the .meta.",
            p.path, p.partner
        ),
        MetaKind::MetaDeletedAssetKept => format!(
            "{} is deleted but {} stays: Unity makes a new one, with a new GUID, and its \
             references break.",
            p.path, p.partner
        ),
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
                [one] => format!("Unity: {}", describe(one)),
                many => format!(
                    "Unity: {} assets and .meta files don't match ({} and more; hover to see \
                     them all).",
                    many.len(),
                    many[0].path
                ),
            };
            let list: Vec<String> = state.meta.iter().take(LISTED).map(describe).collect();
            let more = state.meta.len().saturating_sub(LISTED);
            let fixes = self.meta_fixes();
            let idle = self.busy_op.is_none();
            let r = strip(ui, &t, t.warning, &text, |ui| {
                for (label, action) in fixes {
                    if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
                        acts.push(action);
                    }
                }
            });
            if state.meta.len() > 1 {
                r.on_hover_ui(|ui| {
                    for line in &list {
                        ui.label(line);
                    }
                    if more > 0 {
                        ui.label(format!("and {more} more"));
                    }
                });
            }
        }
        if let Some(problem) = state.hint() {
            let text = format!("This Unity project isn't fully set up for git: {problem}.");
            strip(ui, &t, t.accent, &text, |ui| {
                if ui
                    .button("Don't show again")
                    .on_hover_text("For this repository (gitcraft.unityHint in its config)")
                    .clicked()
                {
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
fn strip(
    ui: &mut Ui,
    t: &Tokens,
    color: egui::Color32,
    text: &str,
    buttons: impl FnOnce(&mut Ui),
) -> egui::Response {
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
        })
        .response
}
