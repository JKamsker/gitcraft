//! The General page's "Portable mode" section: where this copy keeps its files, and whether it
//! uses the system's credential store. Shown only when portable mode is on, or was asked for.

use egui::{RichText, Ui};

use super::layout::{Filter, section};
use super::{Settings, indented_note};
use crate::portable::{self, Mode};
use crate::theme::Tokens;

impl Settings {
    pub(super) fn portable_ui(&mut self, ui: &mut Ui, f: &Filter) {
        let t = Tokens::get(ui.ctx());
        match portable::mode() {
            Mode::Installed => {}
            Mode::Unusable(why) => {
                section(ui, f, "Portable mode", None, |card| {
                    card.row("Folder", "portable usb stick data folder", |ui| {
                        ui.label(RichText::new(why).color(t.danger));
                    });
                });
            }
            Mode::Portable(dir) => {
                section(ui, f, "Portable mode", None, |card| {
                    card.row("Folder", "portable usb stick data folder", |ui| {
                        ui.label(RichText::new(dir.display().to_string()).color(t.text_dim))
                            .on_hover_text(
                                "Settings, open tabs, profiles and their git config files, the \
                                 avatar cache and downloaded updates are kept here, next to \
                                 gitcraft.",
                            );
                    });
                    card.row(
                        "Sign-ins",
                        "portable credential store keyring password remember",
                        |ui| {
                            ui.checkbox(
                                &mut self.portable_credential_store,
                                "Remember sign-ins in this computer's credential store",
                            );
                            indented_note(
                                ui,
                                "Off: SSH passwords and passphrases are asked again after a \
                                 restart, and nothing is left on this computer.",
                            );
                        },
                    );
                });
            }
        }
    }
}
