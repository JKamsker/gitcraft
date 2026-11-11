//! General preferences, desktop behavior and update controls.

use super::file_menus::file_menu_rows;
use super::layout::{self, Card, Filter, section};
use super::{COMBO_WIDTH, Settings, SettingsView, UpdateCheck};
use crate::theme::Tokens;
use egui::{RichText, Ui};

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter, view: &mut SettingsView) {
    let t = Tokens::get(ui.ctx());
    section(
        ui,
        f,
        crate::i18n::text("settings-language-section"),
        None,
        |card| {
            card.row(
                crate::i18n::text("settings-language-label"),
                "language locale translation Sprache",
                |ui| {
                    settings.language_ui(ui);
                },
            );
        },
    );
    section(ui, f, "Quiet celebrations", None, |card| {
        card.row(
            "After success",
            "first commit release tag merge sound motion",
            |ui| {
                ui.checkbox(
                    &mut settings.celebration_motion,
                    crate::i18n::text("interaction-settings-celebration-motion"),
                );
                ui.checkbox(
                    &mut settings.celebration_sound,
                    crate::i18n::text("interaction-settings-celebration-sound"),
                );
                ui.small(crate::i18n::text("interaction-settings-celebration-note"));
                if crate::audio::unavailable() {
                    ui.label(crate::i18n::text("interaction-settings-audio-unavailable"));
                    if ui
                        .button(crate::i18n::text("interaction-settings-retry-audio"))
                        .clicked()
                    {
                        crate::audio::retry();
                    }
                }
            },
        );
    });
    section(ui, f, "Behaviour", None, |card| {
        card.row("Worktrees", "open same tab switch", |ui| {
            ui.checkbox(
                &mut settings.worktrees_in_same_tab,
                crate::i18n::text("interaction-settings-same-tab"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-same-tab-tip"));
        });
        card.row("Notifications", "desktop long operation background", |ui| {
            ui.checkbox(
                &mut settings.notifications,
                crate::i18n::text("interaction-settings-notify-when"),
            );
        });
        card.row(
            "Resident watcher",
            "tray background close recent repositories notifications",
            |ui| {
                ui.checkbox(
                    &mut settings.keep_running_in_tray,
                    crate::i18n::text("interaction-settings-resident"),
                );
                ui.small(crate::i18n::text("interaction-settings-resident-policy"));
                if settings.keep_running_in_tray {
                    match crate::tray::status() {
                        crate::tray::Availability::Ready => {
                            ui.small(crate::i18n::text("interaction-settings-tray-ready"));
                        }
                        crate::tray::Availability::Fallback(why) => {
                            ui.small(crate::i18n::message(
                                "interaction-settings-tray-fallback",
                                &[("why", why.as_str().into())],
                            ));
                        }
                        _ => {
                            ui.small(crate::i18n::text("interaction-settings-tray-starting"));
                        }
                    }
                }
            },
        );
        file_menu_rows(card);
        card.row("Tips", "did you know hints help first time", |ui| {
            settings.tips_ui(ui);
        });
        if let Some(appimage) =
            crate::desktop_entry::appimage().filter(|_| !crate::portable::asked())
        {
            card.row(
                "Applications menu",
                "appimage desktop entry launcher",
                |ui| {
                    if crate::desktop_entry::installed() {
                        if ui
                            .button(crate::i18n::text("interaction-settings-remove-app"))
                            .clicked()
                        {
                            let _ = crate::desktop_entry::uninstall();
                        }
                    } else if ui
                        .button(crate::i18n::text("interaction-settings-add-app"))
                        .clicked()
                    {
                        let _ = crate::desktop_entry::install(&appimage);
                    }
                    layout::note(ui, crate::i18n::text("interaction-settings-appimage-note"));
                },
            );
        }
    });
    settings.portable_ui(ui, f);
    section(ui, f, "Updates", None, |card| {
        card.row("Version", "about release", |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("gitcraft {}", crate::update::CURRENT)).color(t.text_dim),
            );
        });
        card.row("Check now", "check for updates new version look", |ui| {
            ui.horizontal_wrapped(|ui| {
                let checking = view.update == UpdateCheck::Checking;
                let pinned = matches!(view.update, UpdateCheck::Pinned(_));
                if ui
                    .add_enabled(
                        !checking && !pinned,
                        egui::Button::new(crate::i18n::text("interaction-settings-check-updates")),
                    )
                    .on_hover_text(crate::i18n::message(
                        "interaction-settings-update-channel",
                        &[("channel", settings.update_channel.label().into())],
                    ))
                    .on_disabled_hover_text(if pinned {
                        crate::i18n::text("interaction-settings-pinned-tip")
                    } else {
                        crate::i18n::text("interaction-settings-looking")
                    })
                    .clicked()
                {
                    view.check_updates_now = true;
                }
                match &view.update {
                    UpdateCheck::NotYet => {}
                    UpdateCheck::Checking => {
                        ui.add(crate::spinner::Spinner::new());
                        layout::note(ui, crate::i18n::text("interaction-settings-looking"));
                    }
                    UpdateCheck::UpToDate => {
                        layout::note(ui, crate::i18n::text("interaction-settings-newest"));
                    }
                    UpdateCheck::Found(version) => {
                        ui.label(
                            RichText::new(crate::i18n::message(
                                "interaction-settings-update-found",
                                &[("version", version.as_str().into())],
                            ))
                            .color(t.success)
                            .small(),
                        );
                    }
                    UpdateCheck::Failed(e) => {
                        ui.label(RichText::new(e).color(t.danger).small());
                    }
                    // Said in the Pinned row, with its Unpin button.
                    UpdateCheck::Pinned(_) => {}
                }
            });
        });
        rollback_rows(card, view);
        card.row("New versions", "check download github restart", |ui| {
            ui.checkbox(
                &mut settings.check_updates,
                crate::i18n::text("interaction-settings-updates-start"),
            );
            ui.add_enabled_ui(settings.check_updates, |ui| {
                ui.checkbox(
                    &mut settings.download_updates,
                    crate::i18n::text("interaction-settings-updates-download"),
                );
            });
        });
        card.row("Channel", "main dev releases builds", |ui| {
            ui.add_enabled_ui(settings.check_updates, |ui| {
                egui::ComboBox::from_id_salt("settings-update-channel")
                    .width(layout::fit(ui, COMBO_WIDTH))
                    .selected_text(settings.update_channel.label())
                    .show_ui(ui, |ui| {
                        for channel in crate::update::Channel::ALL {
                            ui.selectable_value(
                                &mut settings.update_channel,
                                channel,
                                channel.label(),
                            );
                        }
                    });
            });
        });
    });
}

/// The Updates section's rows for going back to the version the last update replaced, and for
/// the pin that keeps it.
fn rollback_rows(card: &mut Card, view: &mut SettingsView) {
    if let Some(version) = view.previous.clone() {
        let (button, tip) = if view.previous_is_newer {
            (
                crate::i18n::message(
                    "interaction-settings-forward",
                    &[("version", version.as_str().into())],
                ),
                crate::i18n::message(
                    "interaction-settings-forward-tip",
                    &[("version", version.as_str().into())],
                ),
            )
        } else {
            (
                crate::i18n::message(
                    "interaction-settings-back",
                    &[("version", version.as_str().into())],
                ),
                crate::i18n::message(
                    "interaction-settings-back-tip",
                    &[("version", version.as_str().into())],
                ),
            )
        };
        card.row(
            "Previous version",
            "go back forward rollback undo update downgrade",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(button).on_hover_text(tip).clicked() {
                        view.go_back = true;
                    }
                });
            },
        );
    }
    if !view.pinned.is_empty() {
        let pinned = view.pinned.clone();
        card.row("Pinned", "pin stay version unpin", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::message(
                    "interaction-settings-staying",
                    &[("version", pinned.as_str().into())],
                ));
                if ui
                    .button(crate::i18n::text("interaction-settings-unpin"))
                    .on_hover_text(crate::i18n::text("interaction-settings-unpin-tip"))
                    .clicked()
                {
                    view.unpin = true;
                }
            });
        });
    }
}
