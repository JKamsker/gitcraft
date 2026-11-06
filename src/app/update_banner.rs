//! The banner under the tabs when a newer release is out: "Update now" once it is downloaded.

use std::path::Path;

use egui::{RichText, Stroke, Ui};

use super::App;
use crate::icons;
use crate::theme::{self, Tokens};
use crate::update;

impl App {
    pub(super) fn update_banner(&mut self, ui: &mut Ui) {
        let Some(found) = self.update.clone() else {
            return;
        };
        if self.update_later {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let release = &found.release;
        egui::Panel::top("update-banner")
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.separator))
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "cloud-download", 16.0, t.accent);
                    let what = if found.parked.is_some() {
                        "is downloaded, ready to install"
                    } else {
                        "is out"
                    };
                    ui.label(format!(
                        "gitcraft {} {what} (you have {}).",
                        release.version,
                        update::CURRENT
                    ));
                    if let Some(error) = &self.update_error {
                        ui.label(RichText::new(error).color(t.danger));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", "Not now: ask again on the next start")
                            .clicked()
                        {
                            self.update_later = true;
                        }
                        if ui.button("Skip this version").clicked() {
                            self.skipped_version = release.version.clone();
                            self.update = None;
                        }
                        if ui.button("Release notes").clicked() {
                            let _ = open::that_detached(&release.url);
                        }
                        let (label, tip) = match found.parked {
                            Some(_) => (
                                "Update now",
                                "Restart gitcraft as the new version; the open tabs stay",
                            ),
                            // It couldn't be downloaded, or this copy of gitcraft can't replace
                            // itself (a package's, or in a folder of the administrator's).
                            None => ("Download", "Open the release page"),
                        };
                        let button = egui::Button::new(
                            RichText::new(label)
                                .color(t.accent_text)
                                .font(theme::medium(12.0)),
                        )
                        .fill(t.accent);
                        if ui.add(button).on_hover_text(tip).clicked() {
                            match &found.parked {
                                Some(new) => self.update_now(new, ui.ctx()),
                                None => {
                                    let _ = open::that_detached(&release.url);
                                }
                            }
                        }
                    });
                });
            });
    }

    /// Puts the downloaded version `new` in place and closes the window, which saves the tabs;
    /// `main` then starts the new version.
    fn update_now(&mut self, new: &Path, ctx: &egui::Context) {
        match update::install(new) {
            Ok(program) => {
                update::relaunch_after_exit(program);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.update_error = Some(e),
        }
    }
}
