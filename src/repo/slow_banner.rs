//! What is still being read once it has taken longer than it should: a banner above the tab,
//! rather than spinners everywhere from the first moment (they show only after a grace, see
//! `crate::spinner`).

use std::time::{Duration, Instant};

use super::RepoTab;
use crate::theme::Tokens;

/// How long a read may take before the banner says it is still going.
pub(super) const SLOW: Duration = Duration::from_secs(5);

impl RepoTab {
    /// What has been read for longer than [`SLOW`], if anything; else when to look again.
    fn slow_read(&self) -> Result<String, Option<Duration>> {
        let mut next: Option<Duration> = None;
        let mut over = |since: Option<Instant>| match since.map(|at| at.elapsed()) {
            Some(waited) if waited >= SLOW => true,
            Some(waited) => {
                let left = SLOW - waited;
                next = Some(next.map_or(left, |n| n.min(left)));
                false
            }
            None => false,
        };
        if self.loading && over(self.load_started) {
            return Ok(crate::i18n::message(
                "repo-core-slow-load",
                &[("name", self.name.to_string().into())],
            ));
        }
        let detail_pending =
            self.detail.is_none() && self.detail_error.is_none() && self.detail_hash.is_some();
        if detail_pending && over(self.detail_asked) {
            return Ok(crate::i18n::text("repo-core-slow-detail").to_owned());
        }
        Err(next)
    }

    pub(super) fn slow_banner(&self, ui: &mut egui::Ui) {
        let text = match self.slow_read() {
            Ok(text) => text,
            Err(next) => {
                if let Some(next) = next {
                    ui.ctx().request_repaint_after(next);
                }
                return;
            }
        };
        let t = Tokens::get(ui.ctx());
        egui::Panel::top("repo-slow-read")
            .frame(
                egui::Frame::new()
                    .fill(t.warning.gamma_multiply(0.14))
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::icons::inline(ui, "history", 14.0, t.warning);
                    ui.label(egui::RichText::new(text).color(t.text));
                });
            });
    }
}
