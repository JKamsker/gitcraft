//! The banner under the toolbar while HEAD is detached.

use egui::{RichText, Ui};

use crate::icons;
use crate::repo::{Action, Dialog, RepoTab, short};
use crate::theme::{self, Tokens};

/// What the banner says, as the issue asked for it.
pub(super) const SAYS: &str =
    "You're looking at an old version; new commits here belong to no branch.";

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
        ui.horizontal(|ui| {
            icons::inline(ui, "circle-alert", 15.0, t.warning);
            ui.add(
                egui::Label::new(RichText::new(SAYS).color(t.text).font(theme::medium(12.5)))
                    .truncate(),
            )
            .on_hover_text(format!(
                "HEAD is detached at {}: it is on no branch. Commits made now are on no \
                 branch either, and checking out something else leaves them behind.",
                short(&head)
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(branch) = &back
                    && ui
                        .add_enabled(idle, egui::Button::new(format!("Back to {branch}")))
                        .on_hover_text(format!("Check {branch} out again"))
                        .clicked()
                {
                    acts.push(Action::Checkout(branch.clone()));
                }
                if ui
                    .add_enabled(idle, egui::Button::new("Create branch here"))
                    .on_hover_text("A branch at this commit, checked out: new commits go on it")
                    .clicked()
                {
                    acts.push(Action::Dialog(Dialog::create_branch(&head, short(&head))));
                }
            });
        });
    }
}
