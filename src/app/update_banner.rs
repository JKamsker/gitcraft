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
                        paint_update_error(ui, error, self.update_failure.as_ref(), t.danger);
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
        match update::go_back_witnessed() {
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
            Err(failure) => self
                .toasts
                .push(super::panels::Toast::from_go_back_failure(failure)),
        }
    }

    /// Stops staying on the version gone back to: updates are looked for again.
    pub(super) fn unpin(&mut self) {
        self.pinned_version.clear();
        self.skipped_version.clear();
        self.update_discovery.invalidate();
    }

    /// Puts the downloaded version `new` in place and closes the window, which saves the tabs;
    /// `main` then starts the new version.
    fn update_now(&mut self, new: &Path, ctx: &egui::Context) {
        match update::install_witnessed(new) {
            Ok(program) => {
                keep_install_failure(&mut self.update_error, &mut self.update_failure, None);
                update::relaunch_after_exit(program);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(failure) => keep_install_failure(
                &mut self.update_error,
                &mut self.update_failure,
                Some(failure),
            ),
        }
    }
}

/// Keep original raw state and its same-result provenance together, never a translation.
fn keep_install_failure(
    raw: &mut Option<String>,
    receipt: &mut Option<update::Failure>,
    failure: Option<update::Failure>,
) {
    *raw = failure.as_ref().map(|f| f.raw().to_owned());
    *receipt = failure;
}

fn update_error_text(raw: &str, receipt: Option<&update::Failure>) -> String {
    match receipt {
        Some(failure) if failure.raw() == raw => failure.display(),
        _ => raw.to_owned(),
    }
}

fn paint_update_error(
    ui: &mut Ui,
    raw: &str,
    receipt: Option<&update::Failure>,
    color: egui::Color32,
) {
    ui.label(RichText::new(update_error_text(raw, receipt)).color(color));
}

#[cfg(test)]
mod witnessed_display_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn retained_install_failure_repaints_en_de_en_and_raw_receipt_clear_together() {
        let failure = update::Failure::PutNew {
            raw: "Could not put the new version in place: Could not copy the new version".into(),
            failure: update::PlacementFailure::CopyFailed {
                raw: "Could not copy the new version".into(),
            },
        };
        let mut state = (None, None);
        keep_install_failure(&mut state.0, &mut state.1, Some(failure.clone()));
        let before = state.clone();
        crate::i18n::set_language("en-US");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (Option<String>, Option<update::Failure>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                if let Some(raw) = &state.0 {
                    paint_update_error(ui, raw, state.1.as_ref(), egui::Color32::RED);
                }
            },
            state,
        );
        h.run();
        h.get_by_label(failure.raw());
        crate::i18n::set_language("de");
        h.step();
        h.get_by_label("Die neue Version konnte nicht an ihren Platz verschoben werden: Die neue Version konnte nicht kopiert werden");
        assert_eq!(h.state(), &before);
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label(failure.raw());
        assert_eq!(h.state(), &before);
        let state = h.state_mut();
        keep_install_failure(&mut state.0, &mut state.1, None);
        assert_eq!(state, &(None, None));
    }

    #[test]
    fn missing_mismatched_and_opaque_install_receipts_do_not_reclassify_text() {
        crate::i18n::set_language("de");
        let raw = "No previous version of gitcraft is kept";
        let known = update::Failure::NoPrevious { raw: raw.into() };
        assert_eq!(update_error_text(raw, None), raw);
        assert_eq!(
            update_error_text("external lookalike", Some(&known)),
            "external lookalike"
        );
        assert_eq!(
            update_error_text(raw, Some(&update::Failure::Opaque(raw.into()))),
            raw
        );
        let mut state = (Some("old raw".into()), Some(known));
        keep_install_failure(
            &mut state.0,
            &mut state.1,
            Some(update::Failure::Opaque(raw.into())),
        );
        assert_eq!(state.0.as_deref(), Some(raw));
        assert_eq!(state.1.as_ref().map(update::Failure::raw), Some(raw));
        assert_eq!(update_error_text(raw, state.1.as_ref()), raw);
    }
}
