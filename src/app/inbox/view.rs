//! Start-page inbox composition and presentation.

use super::*;

impl App {
    pub(in crate::app) fn inbox_section(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        if self.inbox.poll(&ctx) {
            remember_round(&mut self.inbox_seen, &self.inbox.items, &self.inbox.covered);
        }
        match self.inbox.asked_at.map(|at| at.elapsed()) {
            Some(since) if since <= ASK_EVERY => {
                // Idle, the window draws nothing: it's woken for the next round. While a
                // round runs, each answer wakes it.
                if !self.inbox.busy() {
                    ctx.request_repaint_after(ASK_EVERY.saturating_sub(since));
                }
            }
            _ => {
                let repos = self.inbox_repos(false);
                self.inbox.ask(repos, false, &ctx);
            }
        }
        let mut refresh = false;
        let mut open = None;
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-inbox-heading"))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                if self.inbox.busy() {
                    ui.add(crate::spinner::Spinner::new().size(11.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let tip = crate::i18n::text("interaction-home-ask-again");
                    refresh = icons::small_button(ui, "refresh-cw", tip).clicked();
                });
            });
            let (quiet, failed): (Vec<&Problem>, Vec<&Problem>) = self
                .inbox
                .problems
                .iter()
                .partition(|p| forge_inbox::is_no_login(&p.why));
            let list = waiting(&self.inbox.items, &self.inbox_seen);
            // All clear only when every forge answered.
            if list.is_empty()
                && failed.is_empty()
                && !self.inbox.busy()
                && self.inbox.asked_at.is_some()
            {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-all-clear"))
                        .color(t.text_faint)
                        .small(),
                );
            }
            ui.spacing_mut().item_spacing.y = 2.0;
            let total = list.len();
            let shown = if self.inbox.show_all {
                total
            } else {
                total.min(SHOWN)
            };
            for (item, reasons) in list.into_iter().take(shown) {
                if inbox_row(ui, item, &reasons, &t).clicked() {
                    open = Some(item.clone());
                }
            }
            if total > SHOWN {
                let text = if self.inbox.show_all {
                    crate::i18n::text("interaction-home-fewer").to_owned()
                } else {
                    crate::i18n::message(
                        "interaction-home-show-more",
                        &[("count", (total - SHOWN).into())],
                    )
                };
                if ui.link(RichText::new(text).small()).clicked() {
                    self.inbox.show_all = !self.inbox.show_all;
                }
            }
            let failed = grouped(&failed);
            if !failed.is_empty() {
                let text = crate::i18n::message(
                    "interaction-home-ask-failed",
                    &[("count", failed.len().into())],
                );
                ui.label(RichText::new(text).color(t.text_faint).small())
                    .on_hover_text(failed.join("\n\n"));
            }
            let mut unread = grouped(&quiet);
            unread.extend(self.inbox.cut.iter().cloned());
            if !unread.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-home-unread"))
                        .color(t.text_faint)
                        .small(),
                )
                .on_hover_text(unread.join("\n\n"));
            }
        });
        if refresh {
            let repos = self.inbox_repos(true);
            self.inbox.ask(repos, true, &ctx);
        }
        if let Some(item) = open {
            self.open_from_inbox(item, &ctx);
        }
    }
}
