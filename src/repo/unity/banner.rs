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
        MetaKind::AddedWithoutMeta => crate::i18n::message(
            "repo-workflows-unity-describe-0f6c58c7",
            &[
                ("p-path-1", (p.path).clone().into()),
                ("p-partner-2", (p.partner).clone().into()),
            ],
        ),
        MetaKind::DeletedWithoutMeta => crate::i18n::message(
            "repo-workflows-unity-describe-303d18ef",
            &[
                ("p-path-1", (p.path).clone().into()),
                ("p-partner-2", (p.partner).clone().into()),
            ],
        ),
        MetaKind::MetaWithoutAsset => crate::i18n::message(
            "repo-workflows-unity-describe-c35a1fe3",
            &[
                ("p-path-1", (p.path).clone().into()),
                ("p-partner-2", (p.partner).clone().into()),
            ],
        ),
        MetaKind::MetaDeletedAssetKept => crate::i18n::message(
            "repo-workflows-unity-describe-bae58a49",
            &[
                ("p-path-1", (p.path).clone().into()),
                ("p-partner-2", (p.partner).clone().into()),
            ],
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
                many => crate::i18n::message(
                    "repo-workflows-unity-unity-banners-b610e7b8",
                    &[
                        ("many-len-1", (many.len()).into()),
                        ("many-0-path-2", (many[0].path).clone().into()),
                    ],
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
                        ui.label(crate::i18n::message(
                            "repo-workflows-unity-unity-banners-e1614a83",
                            &[("more", (more).into())],
                        ));
                    }
                });
            }
        }
        if let Some(problem) = state.hint() {
            let text = crate::i18n::message(
                "repo-workflows-unity-unity-banners-3a5a7236",
                &[("problem", (problem).clone().into())],
            );
            strip(ui, &t, t.accent, &text, |ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-unity-unity-banners-d16d035f",
                    ))
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-unity-unity-banners-2a61050c",
                    ))
                    .clicked()
                {
                    acts.push(Action::Unity(UnityAct::DismissHint));
                }
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-unity-unity-banners-8d26b8f8",
                    ))
                    .clicked()
                {
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
