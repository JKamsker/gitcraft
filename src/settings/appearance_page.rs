//! Appearance preferences and composition of the theme editor.

use super::layout::{self, Filter, section};
use super::preview::Preview;
use super::{COMBO_WIDTH, SLIDER_WIDTH, Settings, SettingsView, indented_note};
use crate::theme::Tokens;
use egui::Ui;

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter, view: &mut SettingsView) {
    let t = Tokens::get(ui.ctx());
    section(
        ui,
        f,
        "Theme",
        Some(crate::i18n::text("interaction-settings-theme-scope")),
        |card| settings.themes_ui(card, f, view),
    );
    section(ui, f, "Appearance", None, |card| {
        card.row("Zoom", "scale size interface ui", |ui| {
            ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
            // Zooming moves the slider under the pointer, which would move the value
            // again: while it is dragged, the value waits aside and is applied when
            // it is let go. A click or the arrow keys apply it at once.
            let pending = egui::Id::new("settings-zoom-dragged");
            let mut zoom = ui
                .data(|d| d.get_temp::<f32>(pending))
                .unwrap_or(settings.ui_scale);
            let slider = ui.add(layout::slider(
                egui::Slider::new(&mut zoom, 0.7..=2.0)
                    .step_by(0.05)
                    .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
            ));
            if slider.dragged() {
                ui.data_mut(|d| d.insert_temp(pending, zoom));
            } else {
                ui.data_mut(|d| d.remove::<f32>(pending));
                if slider.changed() || slider.drag_stopped() {
                    settings.ui_scale = zoom;
                }
            }
        });
        if cfg!(any(windows, target_os = "macos")) {
            card.row("Title bar", "window frame caption native tabs", |ui| {
                ui.checkbox(
                    &mut settings.tabs_in_title_bar,
                    crate::i18n::text("interaction-settings-tabs-title"),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-tabs-title-tip"));
            });
        }
        for button in crate::app_buttons::AppButton::ALL {
            let keywords = "button place move title bar bottom footer panel";
            // Row/search identities remain English; Card resolves the visible caption.
            let row_name = match button {
                crate::app_buttons::AppButton::Settings => "Settings",
                crate::app_buttons::AppButton::Palette => "Command palette",
                crate::app_buttons::AppButton::Theme => "Theme",
                crate::app_buttons::AppButton::Manual => "Manual",
            };
            card.row(row_name, keywords, |ui| {
                let layout = settings.button_layout.normalized();
                let was = layout.place_of(button);
                let mut place = was;
                egui::ComboBox::from_id_salt(("settings-button-place", button))
                    .width(layout::fit(ui, COMBO_WIDTH))
                    .selected_text(place.label())
                    .show_ui(ui, |ui| {
                        for p in crate::app_buttons::Place::ALL {
                            ui.selectable_value(&mut place, p, p.label());
                        }
                    })
                    .response
                    .on_hover_text(crate::i18n::text("interaction-settings-button-place"));
                if place != was {
                    settings.button_layout = layout.moved(button, place, usize::MAX);
                }
            });
        }
        card.preview(Preview::CodeColors);
        card.row(
            "Code colours",
            "vs code vscode theme syntax highlighting colors",
            |ui| {
                ui.checkbox(
                    &mut settings.vscode_colors,
                    crate::i18n::text("interaction-settings-vscode-use"),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-vscode-tip"));
                if settings.vscode_colors {
                    let note = match crate::vscode_theme::current() {
                        Some(found) if found.dark == t.dark() => found.name.clone(),
                        Some(found) => crate::i18n::message(
                            if found.dark {
                                "interaction-settings-vscode-dark"
                            } else {
                                "interaction-settings-vscode-light"
                            },
                            &[("theme", found.name.as_str().into())],
                        ),
                        None => crate::i18n::text("interaction-settings-vscode-missing").to_owned(),
                    };
                    indented_note(ui, note);
                }
            },
        );
        card.preview(Preview::Avatars);
        card.row(
            "Avatars",
            "github gitlab forgejo gitea forge gravatar pictures authors",
            |ui| {
                ui.checkbox(
                    &mut settings.avatars,
                    crate::i18n::text("interaction-settings-avatars-load"),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-avatars-tip"));
            },
        );
    });
    section(ui, f, "Diffs and files", None, |card| {
        card.preview(Preview::DiffLayout);
        card.row("Diff layout", "unified split side by side columns", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                ui.radio_value(
                    &mut settings.diff_split,
                    false,
                    crate::i18n::text("interaction-settings-unified"),
                );
                ui.radio_value(
                    &mut settings.diff_split,
                    true,
                    crate::i18n::text("interaction-settings-side-by-side"),
                );
            });
        });
        card.preview(Preview::DiffContext);
        card.row(
            "Diff context",
            "whole file lines around changes hunks fold",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.radio_value(
                        &mut settings.diff_whole_file,
                        false,
                        crate::i18n::text("interaction-settings-around-changes"),
                    );
                    ui.radio_value(
                        &mut settings.diff_whole_file,
                        true,
                        crate::i18n::text("interaction-settings-whole-file"),
                    );
                });
            },
        );
        card.preview(Preview::FileList);
        card.row_hint(
            "Changed files",
            crate::i18n::text("commit-details-files-setting-hint"),
            "tree folders list flat path selected commit",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.radio_value(
                        &mut settings.files_as_tree,
                        true,
                        crate::i18n::text("interaction-settings-folders"),
                    );
                    ui.radio_value(
                        &mut settings.files_as_tree,
                        false,
                        crate::i18n::text("interaction-settings-one-list"),
                    );
                });
            },
        );
    });
}
