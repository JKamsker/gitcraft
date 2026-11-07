//! Settings → General → Tips: the "Did you know" line and the first-time hints, on or off, and
//! the hints seen brought back.

use egui::Ui;

use super::{Settings, layout};

impl Settings {
    pub(super) fn tips_ui(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.tips, "Show tips").on_hover_text(
                "A \u{201c}Did you know\u{201d} line under the graph, and a one-line hint the \
                     first time you do something that has a shortcut",
            );
            let seen = self.seen_hints.len();
            if seen > 0 {
                if ui
                    .add_enabled(self.tips, egui::Button::new("Show the hints again"))
                    .on_hover_text("The first-time hints already shown come back once more")
                    .clicked()
                {
                    self.seen_hints.clear();
                }
                layout::note(ui, format!("{seen} hints seen"));
            }
        });
    }
}
