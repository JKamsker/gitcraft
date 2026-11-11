//! Drawing the card: the problem, what was found, the fix and its confirmation.

use egui::{RichText, Ui};

use super::{CardEvent, ErrorCard, Stage, text};
use crate::icons;
use crate::repo::widgets::primary_button;
use crate::theme::{self, Tokens};

/// Tallest the card's text gets before it scrolls.
const MAX_BODY: f32 = 220.0;

impl ErrorCard {
    /// Draws the card; `retry` when what failed can be run again from it. What its buttons
    /// ask of the owner comes back.
    pub fn ui(&mut self, ui: &mut Ui, retry: bool) -> Option<CardEvent> {
        self.poll();
        let retry = retry && self.can_retry();
        let t = Tokens::get(ui.ctx());
        let mut event = None;
        let accent = match self.stage {
            Stage::Failed(_) => t.danger,
            Stage::Done { .. } | Stage::Undone(_) => t.success,
            _ => t.warning,
        };
        egui::Frame::new()
            .fill(t.card)
            .stroke(egui::Stroke::new(1.0, accent))
            .corner_radius(t.radius)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.header(ui, accent) {
                    event = Some(CardEvent::Close);
                }
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt(("error_card", &self.dir))
                    .max_height(MAX_BODY)
                    .auto_shrink([false, true])
                    .show(ui, |ui| self.body(ui));
                ui.add_space(6.0);
                if let Some(e) = self.actions(ui, retry) {
                    event = Some(e);
                }
            });
        if event == Some(CardEvent::Retry) {
            self.retried();
        }
        event
    }

    /// The icon, the title and the close button; whether that was clicked.
    fn header(&self, ui: &mut Ui, accent: egui::Color32) -> bool {
        let t = Tokens::get(ui.ctx());
        let mut close = false;
        ui.horizontal(|ui| {
            icons::inline(ui, "circle-alert", 16.0, accent);
            ui.add(
                egui::Label::new(
                    RichText::new(text::title(&self.problem))
                        .font(theme::semibold(14.0))
                        .color(t.text),
                )
                .wrap(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button =
                    icons::small_button(ui, "x", crate::i18n::text("interaction-error-close"));
                // Named for screen readers (and tests): the icon has no text.
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        crate::i18n::text("interaction-error-close"),
                    )
                });
                close = button.clicked();
            });
        });
        close
    }

    fn body(&self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let para = |ui: &mut Ui, s: &str, color| {
            ui.add(egui::Label::new(RichText::new(s).color(color)).wrap());
        };
        for p in text::explanation(&self.problem, self.place) {
            para(ui, &p, t.text_dim);
        }
        match &self.found {
            None => {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(
                        RichText::new(crate::i18n::text("interaction-error-looking"))
                            .color(t.text_faint),
                    );
                });
            }
            Some(Err(e)) => para(
                ui,
                &crate::i18n::message(
                    "interaction-error-look-error",
                    &[("error", e.as_str().into())],
                ),
                t.danger,
            ),
            Some(Ok(found)) => {
                for note in &found.notes {
                    let color = if note.warn { t.warning } else { t.text };
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new("•").color(t.text_faint));
                        para(ui, &note.text, color);
                    });
                }
            }
        }
        egui::CollapsingHeader::new(
            RichText::new(crate::i18n::text("interaction-error-git-said")).color(t.text_faint),
        )
        .id_salt(("error_card_said", &self.dir))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(self.said.trim())
                        .font(theme::mono(11.5))
                        .color(t.text_dim),
                )
                .wrap(),
            );
        });
    }

    /// The buttons of the stage the fix is at.
    fn actions(&mut self, ui: &mut Ui, retry: bool) -> Option<CardEvent> {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let mut event = None;
        let retry_button = |ui: &mut Ui, event: &mut Option<CardEvent>| {
            if retry && button(ui, crate::i18n::text("interaction-error-retry")).clicked() {
                *event = Some(CardEvent::Retry);
            }
        };
        match self.stage.clone() {
            Stage::Idle => {
                ui.horizontal(|ui| {
                    if let Some(fix) = self.offered()
                        && primary_button(ui, fix.button(), true, 0.0).clicked()
                    {
                        self.ask();
                    }
                    retry_button(ui, &mut event);
                });
            }
            Stage::Confirming => {
                let Some(fix) = self.offered() else {
                    self.back(&ctx);
                    return None;
                };
                let (label, confirm) = (fix.button(), fix.confirm());
                ui.add(egui::Label::new(RichText::new(confirm).color(t.text)).wrap());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if primary_button(ui, label, true, 0.0).clicked() {
                        self.confirm(&ctx);
                    }
                    if button(ui, crate::i18n::text("interaction-error-cancel")).clicked() {
                        self.back(&ctx);
                    }
                });
            }
            Stage::Working => {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new());
                    ui.label(
                        RichText::new(crate::i18n::text("interaction-error-working"))
                            .color(t.text_dim),
                    );
                });
            }
            Stage::Done { what, undo } => {
                ui.add(egui::Label::new(RichText::new(what).color(t.success)).wrap());
                ui.horizontal(|ui| {
                    retry_button(ui, &mut event);
                    if undo.is_some()
                        && button(ui, crate::i18n::text("interaction-error-undo")).clicked()
                    {
                        self.undo(&ctx);
                    }
                });
            }
            Stage::Undone(what) => {
                ui.add(egui::Label::new(RichText::new(what).color(t.text_dim)).wrap());
            }
            Stage::Failed(e) => {
                ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
                if button(ui, crate::i18n::text("interaction-error-back")).clicked() {
                    self.back(&ctx);
                }
            }
        }
        self.earlier_ui(ui);
        event
    }

    /// The fix of the card this one replaced, while it can still be undone here.
    fn earlier_ui(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        if let Some(what) = self.earlier().map(str::to_owned) {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "interaction-error-earlier",
                        &[("what", what.as_str().into())],
                    ))
                    .color(t.text_faint),
                );
                if button(ui, crate::i18n::text("interaction-error-undo-earlier")).clicked() {
                    self.undo_earlier(&ctx);
                }
            });
        } else if let Some(done) = self.earlier_undone() {
            let (text, color) = match done {
                Ok(what) => (what.clone(), t.text_dim),
                Err(e) => (e.clone(), t.danger),
            };
            ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
        }
    }
}

/// A secondary button as tall as [`primary_button`], for the rows they share.
fn button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(egui::vec2(0.0, 30.0)))
}
