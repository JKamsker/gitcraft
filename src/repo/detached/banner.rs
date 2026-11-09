//! The banner under the toolbar while HEAD is detached.

use egui::{RichText, Ui};

use crate::icons;
use crate::repo::{Action, Dialog, RepoTab, short};
use crate::theme::{self, Tokens};

/// What the banner says, as the issue asked for it.
pub(super) const SAYS: &str = "repo-workflows-detached--bff74cc9";

impl RepoTab {
    /// The banner, in a panel of its own under the toolbar, when it shows: other banners (a
    /// bisect's…) stack with it in panels of their own.
    pub(in crate::repo) fn detached_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if !self.detached_banner_shows() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        egui::Panel::top("repo_detached")
            .frame(
                egui::Frame::new()
                    .fill(t.warning.gamma_multiply(0.14))
                    .inner_margin(egui::Margin::symmetric(12, 6))
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| self.detached_banner(ui, acts));
    }

    fn detached_banner(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none() && !self.detached.checking;
        let head = self.refs.head.clone();
        let back = self.back_to().map(str::to_owned);
        // The buttons first, from the right: the text takes what is left, cut short if need be.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(branch) = &back
                && ui
                    .add_enabled(
                        idle,
                        egui::Button::new(crate::i18n::message(
                            "repo-workflows-detached-detached-banner-f76b5669",
                            &[("branch", (branch).clone().into())],
                        )),
                    )
                    .on_hover_text(crate::i18n::message(
                        "repo-workflows-detached-detached-banner-f5cd526a",
                        &[("branch", (branch).clone().into())],
                    ))
                    .clicked()
            {
                acts.push(Action::Checkout(branch.clone()));
            }
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-detached-detached-banner-15bbfb59",
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-detached-detached-banner-f16cf1bf",
                ))
                .clicked()
            {
                acts.push(Action::Dialog(Dialog::create_branch(&head, short(&head))));
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                icons::inline(ui, "circle-alert", 15.0, t.warning);
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::i18n::text(SAYS))
                            .color(t.text)
                            .font(theme::medium(12.5)),
                    )
                    .truncate(),
                )
                .on_hover_text(crate::i18n::message(
                    "repo-workflows-detached-detached-banner-3d7f6bc2",
                    &[("short-head-1", (short(&head)).into())],
                ));
            });
        });
    }
}
