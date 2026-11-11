//! Saved commit identities and their editor.

use super::layout::{self, Filter, section};
use super::{SavedIdentity, Settings};
use egui::Ui;

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter) {
    section(
        ui,
        f,
        "Identities",
        Some(crate::i18n::text("interaction-settings-identities-intro")),
        |card| {
            let mut remove = None;
            for (i, p) in settings.identities.iter_mut().enumerate() {
                let found_by = format!("{} {} {}", p.label, p.name, p.email);
                card.full(&found_by, |ui| {
                    ui.horizontal(|ui| {
                        // Shares of the row, less the gaps and the remove button.
                        let room = ui.available_width() - 60.0;
                        ui.add(
                            layout::field(&mut p.label)
                                .hint_text(crate::i18n::text("interaction-settings-work-hint"))
                                .desired_width((room * 0.2).min(110.0)),
                        );
                        ui.add(
                            layout::field(&mut p.name)
                                .hint_text(crate::i18n::text("interaction-settings-name-hint"))
                                .desired_width((room * 0.32).min(170.0)),
                        );
                        // The email takes what is left, the remove button at the end.
                        let rest = (ui.available_width() - 32.0).max(60.0);
                        ui.add(
                            layout::field(&mut p.email)
                                .hint_text("email@example.com")
                                .desired_width(rest),
                        );
                        if crate::icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-settings-remove"),
                        )
                        .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                settings.identities.remove(i);
            }
            card.full("add new name email user profile", |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(crate::i18n::text("interaction-settings-add-identity"))
                        .clicked()
                    {
                        settings.identities.push(SavedIdentity::default());
                    }
                    if settings.identities.is_empty() {
                        layout::note(ui, crate::i18n::text("interaction-settings-none-yet"));
                    }
                });
            });
        },
    );
}
