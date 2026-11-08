//! Settings → General → Tips: the "Did you know" line and the first-time hints, on or off, and
//! the hints seen brought back.

use egui::Ui;

use super::{Settings, layout};

impl Settings {
    pub(super) fn tips_ui(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.tips,
                crate::i18n::text("interaction-settings-tips-show"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-tips-tooltip"));
            let seen = self.seen_hints.len();
            if seen > 0 {
                if ui
                    .add_enabled(
                        self.tips,
                        egui::Button::new(crate::i18n::text("interaction-settings-hints-again")),
                    )
                    .on_hover_text(crate::i18n::text("interaction-settings-hints-again-tip"))
                    .clicked()
                {
                    self.seen_hints.clear();
                }
                layout::note(
                    ui,
                    crate::i18n::message(
                        "interaction-settings-hints-seen",
                        &[("count", seen.into())],
                    ),
                );
            }
        });
    }
}
