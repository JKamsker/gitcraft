//! Default shell and terminal font preferences.

use super::layout::{self, Filter, section};
use super::preview::Preview;
use super::{COMBO_WIDTH, SLIDER_WIDTH, Settings, parse_localized_number};
use egui::Ui;

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter) {
    section(ui, f, "Terminal", None, |card| {
        card.preview(Preview::Terminal);
        card.row("Default shell", "bash zsh powershell cmd fish", |ui| {
            let shells = crate::terminal::available_shells();
            let current = shells
                .iter()
                .find(|s| s.program == settings.shell)
                .map(|s| s.label.clone())
                .unwrap_or_else(|| {
                    crate::i18n::text("interaction-settings-system-default").to_owned()
                });
            egui::ComboBox::from_id_salt("settings-shell")
                .width(layout::fit(ui, COMBO_WIDTH))
                .selected_text(current)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut settings.shell,
                        String::new(),
                        crate::i18n::text("interaction-settings-system-default"),
                    );
                    for s in &shells {
                        ui.selectable_value(&mut settings.shell, s.program.clone(), &s.label);
                    }
                });
        });
        card.preview(Preview::Terminal);
        card.row("Font size", "text", |ui| {
            ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
            ui.add(layout::slider(
                egui::Slider::new(&mut settings.terminal_font, 9.0..=20.0)
                    .step_by(0.5)
                    .max_decimals(1)
                    .custom_formatter(|value, _| crate::i18n::number(value))
                    .custom_parser(parse_localized_number)
                    .suffix(" pt"),
            ));
        });
    });
}
