//! A known git failure of the tab's, explained under the toolbar with its fix (see
//! [`crate::error_card`]).

use egui::Ui;

use super::RepoTab;
use crate::error_card::{CardEvent, ErrorCard};
use crate::theme::Tokens;

/// What a failed read of the repository is called on the card.
pub(super) const RELOAD: &str = "Reload";

impl RepoTab {
    /// Shows the card for `error` (what git said when `label` failed), if it is a failure
    /// gitcraft knows. Another failure leaves a card that is there as it is, and so does the
    /// same failure again (a reload the watcher started, Try again failing the same way): the
    /// question it asks, or the undo it offers, stays.
    pub(super) fn explain_failure(&mut self, error: &str, label: &str, ctx: &egui::Context) {
        if let Some(card) = self.error_card.as_mut().filter(|c| c.is_about(error)) {
            card.failed_again();
            return;
        }
        if let Some(card) = ErrorCard::for_error(error, &self.path, label, ctx) {
            self.error_card = Some(card.replacing(self.error_card.take()));
        }
    }

    /// The card, between the toolbar and the rest, while there is one.
    pub(super) fn error_card_panel(&mut self, ui: &mut Ui) {
        let Some(card) = &mut self.error_card else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let retry = self.busy_op.is_none()
            && (card.label() == RELOAD
                || self
                    .last_op
                    .as_ref()
                    .is_some_and(|op| op.label == card.label()));
        let mut event = None;
        egui::Panel::top("repo_error_card")
            .frame(
                egui::Frame::new()
                    .fill(t.dock)
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| event = card.ui(ui, retry));
        let ctx = ui.ctx().clone();
        match event {
            Some(CardEvent::Close) => self.error_card = None,
            Some(CardEvent::Retry) => self.retry_failed(&ctx),
            None => {}
        }
    }

    /// Runs what the card is about again: the read of the repository, or the operation.
    fn retry_failed(&mut self, ctx: &egui::Context) {
        let Some(card) = &self.error_card else { return };
        if card.label() == RELOAD {
            if !card.fixed() {
                self.error_card = None;
            }
            self.reload(ctx);
        } else {
            self.run_last_op_again(ctx);
        }
    }
}

#[cfg(test)]
#[path = "error_help_tests.rs"]
mod tests;
