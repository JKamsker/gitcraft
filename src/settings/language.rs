use super::Settings;

impl Settings {
    pub(super) fn language_ui(&mut self, ui: &mut egui::Ui) {
        let selected = match self.language.as_str() {
            "" => crate::i18n::text("settings-language-system"),
            "en-US" => "English",
            "de" => "Deutsch",
            other => other,
        }
        .to_owned();
        let before = self.language.clone();
        egui::ComboBox::from_id_salt("interface-language")
            .selected_text(&selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.language,
                    String::new(),
                    crate::i18n::text("settings-language-system"),
                );
                ui.selectable_value(&mut self.language, "en-US".to_owned(), "English");
                ui.selectable_value(&mut self.language, "de".to_owned(), "Deutsch");
            })
            .response
            .widget_info(|| {
                let mut info = egui::WidgetInfo::labeled(
                    egui::WidgetType::ComboBox,
                    ui.is_enabled(),
                    crate::i18n::text("settings-language-label"),
                );
                info.current_text_value = Some(selected.clone());
                info
            });
        if self.language != before && crate::i18n::set_language(&self.language) {
            ui.ctx().request_repaint();
        }
        ui.small(crate::i18n::text("settings-language-note"));
        ui.small(crate::i18n::message(
            "settings-language-example",
            &[
                ("number", crate::i18n::number(1234.5).into()),
                ("date", crate::i18n::date_iso("2024-12-31").into()),
            ],
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_language_control_changes_selection_and_visible_catalog_without_native_objects() {
        crate::i18n::set_language("en-US");
        let mut h =
            Harness::new_ui_state(|ui, settings| settings.language_ui(ui), Settings::default());
        h.get_by_label("Interface language").click();
        h.run();
        h.get_by_label("Deutsch").click();
        h.run();
        assert_eq!(h.state().language, "de");
        h.get_by_label("Beispiel: 1.234,5 · 31.12.2024");
        h.get_by_label("Sprache der Oberfläche").click();
        h.run();
        h.get_by_label("English").click();
        h.run();
        assert_eq!(h.state().language, "en-US");
        h.get_by_label("Example: 1,234.5 · 12/31/2024");
    }
}
