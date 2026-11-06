//! Where tips show: the "Did you know" line at the bottom of the graph area, and the
//! first-time hint at the right end of the toolbar.

use std::time::{Instant, SystemTime};

use egui::{RichText, Ui};

use super::tips;
use crate::icons;
use crate::repo::RepoTab;
use crate::settings::Settings;
use crate::theme::Tokens;

impl RepoTab {
    /// The "Did you know" line, at the bottom of `ui` (the graph area): the day's tip, a
    /// button to the next one and one to turn tips off for good. Nothing once they are off.
    pub(in crate::repo) fn tip_bar(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !Settings::get(&ctx).tips {
            return;
        }
        let seed = tips::day_seed(SystemTime::now()).wrapping_add(self.guide.tip_offset);
        let Some(tip) = tips::pick(tips::manual_tips(), seed) else {
            return;
        };
        let t = Tokens::get(&ctx);
        let (mut next, mut off) = (false, false);
        egui::Panel::bottom("did_you_know")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(t.dock)
                    .inner_margin(egui::Margin::symmetric(10, 3))
                    .stroke(egui::Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    icons::inline(ui, "book-open", 13.0, t.text_faint);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        off = icons::small_button(
                            ui,
                            "x",
                            "Don't show tips any more (Settings → General → Tips brings them back)",
                        )
                        .clicked();
                        next = icons::small_button(ui, "chevron-right", "Next tip").clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new("Did you know?")
                                    .color(t.text_dim)
                                    .small()
                                    .strong(),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(tip).color(t.text_faint).small())
                                    .truncate(),
                            )
                            .on_hover_text(tip);
                        });
                    });
                });
            });
        if next {
            self.guide.tip_offset = self.guide.tip_offset.wrapping_add(1);
        }
        if off {
            let mut settings = Settings::get(&ctx);
            settings.tips = false;
            settings.set(&ctx);
            self.guide.hints.close();
        }
    }

    /// The first-time hint up, if any, with a button closing it (drawn right to left).
    pub(in crate::repo) fn hint_status(&mut self, ui: &mut Ui) {
        let Some((hint, left)) = self.guide.hints.current(Instant::now()) else {
            return;
        };
        if !Settings::get(ui.ctx()).tips {
            self.guide.hints.close();
            return;
        }
        // Gone by itself once its time is up.
        ui.ctx().request_repaint_after(left);
        let t = Tokens::get(ui.ctx());
        if icons::small_button(ui, "x", "Close the tip").clicked() {
            self.guide.hints.close();
        }
        ui.label(RichText::new(hint.text()).color(t.accent));
        ui.add_space(8.0);
    }
}
