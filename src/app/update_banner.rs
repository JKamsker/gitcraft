//! The banner under the tabs when a newer release is out: crate::i18n::text("interaction-dialogs-update-now") once it is downloaded.

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
                    ui.label(crate::i18n::message(
                        "interaction-dialogs-update-state",
                        &[
                            ("version", release.version.as_str().into()),
                            ("current", update::CURRENT.into()),
                            (
                                "state",
                                if found.parked.is_some() {
                                    "ready"
                                } else {
                                    "out"
                                }
                                .into(),
                            ),
                        ],
                    ));
                    if let Some(error) = &self.update_error {
                        ui.label(RichText::new(crate::git::display_error(error)).color(t.danger));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(
                            ui,
                            "x",
                            crate::i18n::text("interaction-dialogs-not-now"),
                        )
                        .clicked()
                        {
                            self.update_later = true;
                        }
                        if ui
                            .button(crate::i18n::text("interaction-dialogs-skip"))
                            .clicked()
                        {
                            self.skipped_version = release.version.clone();
                            self.update = None;
                        }
                        if ui
                            .button(crate::i18n::text("interaction-dialogs-release-notes"))
                            .clicked()
                        {
                            let _ = open::that_detached(&release.url);
                        }
                        let (label, tip) = match found.parked {
                            Some(_) => (
                                crate::i18n::text("interaction-dialogs-update-now"),
                                crate::i18n::text("interaction-dialogs-restart"),
                            ),
                            // It couldn't be downloaded, or this copy of gitcraft can't replace
                            // itself (a package's, or in a folder of the administrator's).
                            None => (
                                crate::i18n::text("interaction-dialogs-download"),
                                crate::i18n::text("interaction-dialogs-release-page"),
                            ),
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

    /// Goes back to the version the last update replaced ("Go back to 0.3.3" in the settings)
    /// and pins it, or forward again to the one gone back from, unpinned: the window closes, which saves the tabs, and `main` starts that version.
    pub(super) fn go_back(&mut self, ctx: &egui::Context) {
        match update::go_back() {
            Ok((program, previous)) => {
                if previous.is_newer() {
                    // Forward again, to the version gone back from: no longer staying put.
                    self.pinned_version.clear();
                    self.skipped_version.clear();
                } else {
                    self.pinned_version = previous
                        .version
                        .unwrap_or_else(|| update::PIN_THIS.to_owned());
                    // Versions before pins existed skip this one instead.
                    self.skipped_version = update::CURRENT.to_owned();
                }
                self.update = None;
                update::relaunch_after_exit(program);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.toasts.push(super::panels::Toast::new(
                crate::i18n::message(
                    "interaction-dialogs-go-back",
                    &[("error", crate::git::display_error(&e).into())],
                ),
                true,
            )),
        }
    }

    /// Stops staying on the version gone back to: updates are looked for again.
    pub(super) fn unpin(&mut self) {
        self.pinned_version.clear();
        self.skipped_version.clear();
        self.update_looked = None;
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
