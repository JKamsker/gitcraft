//! The General page's "Portable mode" section: where this copy keeps its files, and whether it
//! uses the system's credential store. Shown only when portable mode is on, or was asked for.

use egui::{RichText, Ui};

use super::layout::{Card, Filter, section};
use super::{Settings, indented_note};
use crate::portable::{self, Mode};
use crate::theme::Tokens;

impl Settings {
    pub(super) fn portable_ui(&mut self, ui: &mut Ui, f: &Filter) {
        let t = Tokens::get(ui.ctx());
        let (text, color, hint) = match portable::mode() {
            Mode::Installed => return,
            Mode::Unusable(why) => (why.clone(), t.danger, None),
            Mode::Portable(dir) => (
                dir.display().to_string(),
                t.text_dim,
                Some(crate::i18n::text("interaction-settings-portable-folder")),
            ),
        };
        section(ui, f, "Portable mode", None, |card| {
            card.row("Folder", "portable usb stick data folder", |ui| {
                let label = ui.label(RichText::new(&text).color(color));
                if let Some(hint) = hint {
                    label.on_hover_text(hint);
                }
            });
            self.credential_store_row(card);
        });
    }

    fn credential_store_row(&mut self, card: &mut Card) {
        card.row(
            "Sign-ins",
            "portable credential store keyring password remember",
            |ui| {
                ui.checkbox(
                    &mut self.portable_credential_store,
                    crate::i18n::text("interaction-settings-portable-credentials"),
                );
                indented_note(
                    ui,
                    crate::i18n::text("interaction-settings-portable-credentials-note"),
                );
            },
        );
    }
}
