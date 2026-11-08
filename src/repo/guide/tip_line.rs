//! Where tips show: the "Did you know" line at the bottom of the graph area, and the
//! first-time hint at the right end of the toolbar.

use std::time::{Instant, SystemTime};

use egui::{RichText, Ui};

use super::hints::HintState;
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
                        off = icons::small_button(ui, "eye-off", crate::i18n::text(TURN_OFF_TIP))
                            .clicked();
                        ui.add_space(8.0);
                        next = icons::small_button(
                            ui,
                            "chevron-right",
                            crate::i18n::text("repo-workflows-guide-tip-bar-c1567088"),
                        )
                        .clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(crate::i18n::text(
                                    "repo-workflows-guide-tip-bar-3e9b3541",
                                ))
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
            self.turn_tips_off(&ctx);
        }
    }

    /// The tips and the first-time hints off, and the status line saying how to get them back.
    pub(in crate::repo) fn turn_tips_off(&mut self, ctx: &egui::Context) {
        let mut settings = Settings::get(ctx);
        settings.tips = false;
        settings.set(ctx);
        self.guide.hints.close();
        self.status = Some(crate::repo::Notice::plain((TIPS_OFF.to_owned(), false)));
    }

    /// The first-time hint up, if any, with a button closing it (drawn right to left, left of
    /// the status line). Not while an operation runs or the status line has an error: those
    /// keep the room.
    pub(in crate::repo) fn hint_status(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let mut settings = Settings::get(&ctx);
        if !settings.tips {
            self.guide.hints.close();
            return;
        }
        let failed = self.status.as_deref().is_some_and(|(_, err)| *err);
        if self.busy_op.is_some() || failed {
            return;
        }
        let (hint, left) = match self.guide.hints.state(Instant::now()) {
            HintState::None => return,
            HintState::Settling(wait) => {
                ctx.request_repaint_after(wait);
                return;
            }
            HintState::Showing(hint, left) => (hint, left),
        };
        if self.guide.hints.drawn(&mut settings) {
            settings.set(&ctx);
        }
        // Gone by itself once its time is up.
        ctx.request_repaint_after(left);
        let t = Tokens::get(&ctx);
        if icons::small_button(
            ui,
            "x",
            crate::i18n::text("repo-workflows-guide-hint-status-2c0ee188"),
        )
        .clicked()
        {
            self.guide.hints.close();
        }
        let text = hint.text();
        // At most half of what is left, so the branch on the left keeps its room.
        let width = (ui.available_width() / 2.0).max(0.0);
        ui.allocate_ui(egui::vec2(width, ui.available_height()), |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(&text).color(t.accent)).truncate())
                    .on_hover_text(&text);
            });
        });
        ui.add_space(8.0);
    }
}

/// What the ✕ of the tips line does.
const TURN_OFF_TIP: &str = "repo-workflows-guide--b7acfeab";

/// What the status line says once tips are turned off.
pub(crate) const TIPS_OFF: &str = "Tips turned off: Settings → General → Tips turns them back on";
