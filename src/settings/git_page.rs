//! Git workflow preferences and external tool integration.

use super::layout::{self, Card, Filter, section};
use super::preview::Preview;
use super::{
    COMBO_WIDTH, FIELD_WIDTH, Settings, external_tools, indented_note, parse_localized_number,
};
use crate::theme::Tokens;
use egui::{RichText, Ui};

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter) {
    section(ui, f, "Git", None, |card| {
        card.row(
            "Fetch in the background",
            "auto fetch interval minutes remote",
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut settings.auto_fetch_minutes)
                            .range(0..=240)
                            .custom_formatter(|value, _| crate::i18n::number(value))
                            .custom_parser(parse_localized_number)
                            .suffix(" min"),
                    );
                    layout::note(ui, crate::i18n::text("interaction-settings-fetch-off"));
                });
            },
        );
        card.preview(Preview::Pull);
        card.row("Offline", "offline metered network data roaming", |ui| {
            ui.checkbox(
                &mut settings.offline,
                crate::i18n::text("interaction-settings-offline"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-offline-tip"));
            ui.checkbox(
                &mut settings.pause_on_metered,
                crate::i18n::text("interaction-settings-metered"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-metered-tip"));
            if let Some(pause) = crate::network::background_paused() {
                indented_note(
                    ui,
                    crate::i18n::message(
                        "interaction-settings-network-paused",
                        &[("reason", pause.why().into())],
                    ),
                );
            }
        });
        card.row("Pull", "fast-forward rebase merge", |ui| {
            egui::ComboBox::from_id_salt("settings-pull")
                .width(layout::fit(ui, COMBO_WIDTH))
                .selected_text(settings.pull_mode.label())
                .show_ui(ui, |ui| {
                    for mode in crate::repo::pull::PullMode::ALL {
                        ui.selectable_value(&mut settings.pull_mode, mode, mode.label());
                    }
                });
        });
        card.row("Submodules", "update recursive", |ui| {
            ui.checkbox(
                &mut settings.keep_submodules,
                crate::i18n::text("interaction-settings-submodules-update"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-submodules-tip"));
        });
        card.row(
            "Stale branches",
            "stale branch cleanup old days merged delete",
            |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut settings.stale_branch_days)
                            .range(0..=3650)
                            .custom_formatter(|value, _| crate::i18n::number(value))
                            .custom_parser(parse_localized_number)
                            .suffix(crate::i18n::text("interaction-settings-days")),
                    )
                    .on_hover_text(crate::i18n::text("interaction-settings-stale-tip"));
                    layout::note(ui, crate::i18n::text("interaction-settings-stale-off"));
                });
            },
        );
        card.row_hint(
            "Editor command",
            crate::i18n::text("interaction-settings-editor-hint"),
            "code vs code subl idea open",
            |ui| {
                ui.add(
                    layout::field(&mut settings.editor)
                        .hint_text("code")
                        .desired_width(layout::fit(ui, FIELD_WIDTH)),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-editor-tip"));
            },
        );
    });
    section(ui, f, "Diffs", None, |card| settings.diffs_ui(card));
    section(
        ui,
        f,
        "External tools",
        Some(external_tools::intro()),
        |card| settings.external_tools_ui(card),
    );
    section(
        ui,
        f,
        "Mergiraf",
        Some(crate::i18n::text("interaction-settings-mergiraf-intro")),
        |card| mergiraf_ui(settings, card),
    );
    section(
        ui,
        f,
        "Git file tools",
        Some(crate::i18n::text("interaction-settings-git-tools-intro")),
        |card| {
            card.row(
                "Register gitcraft",
                "difftool mergetool compare files standalone global config",
                crate::standalone::registration::settings_ui,
            );
        },
    );
    section(
        ui,
        f,
        "Remote machines",
        Some(crate::i18n::text("interaction-settings-remote-intro")),
        |card| {
            card.row_hint(
                "SSH sign-ins",
                crate::i18n::text("interaction-settings-remember-hint"),
                "ssh password passphrase forget saved credential keyring wsl remote",
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        // The system's store can take a while (the Secret Service
                        // is another process): not on the window's thread.
                        if ui
                            .button(crate::i18n::text("interaction-settings-forget-logins"))
                            .on_hover_text(crate::i18n::text("interaction-settings-forget-tip"))
                            .clicked()
                        {
                            std::thread::spawn(crate::remote::askpass::forget_saved);
                        }
                    });
                },
            );
        },
    );
}

/// Mergiraf: whether it is installed (and a button to install it), whether to offer it in
/// the conflict editor, and whether git merges with it.
fn mergiraf_ui(settings: &mut Settings, card: &mut Card) {
    use crate::mergiraf::{self, Job};
    let ctx = card.ctx();
    let t = Tokens::get(&ctx);
    let state = mergiraf::state(&ctx);
    let job = mergiraf::job();
    let busy = matches!(job, Job::Running(_));
    card.row("Installed", "install version download codeberg", |ui| {
        ui.horizontal(|ui| {
            match &state.found {
                Some(found) => {
                    let what = if found.own {
                        crate::i18n::message(
                            "interaction-settings-mergiraf-version",
                            &[("version", found.version.as_str().into())],
                        )
                    } else {
                        found.version.clone()
                    };
                    ui.label(RichText::new(what).color(t.text_dim));
                }
                None => {
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(crate::i18n::text("interaction-settings-install")),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-mergiraf-download"))
                        .clicked()
                    {
                        mergiraf::install(ui.ctx());
                    }
                    ui.hyperlink_to(
                        crate::i18n::text("interaction-settings-other-ways"),
                        mergiraf::DOCS,
                    );
                    if matches!(job, Job::Idle) {
                        layout::note(ui, crate::i18n::text("interaction-settings-not-installed"));
                    }
                }
            }
            match &job {
                Job::Idle => {}
                Job::Running(doing) => {
                    ui.add(crate::spinner::Spinner::new());
                    layout::note(ui, *doing);
                }
                Job::Done(done) => {
                    ui.label(RichText::new(done).color(t.success).small());
                }
                Job::Failed(e) => {
                    ui.label(RichText::new(e).color(t.danger).small());
                }
            }
        });
    });
    card.row("Conflict editor", "offer answer resolve", |ui| {
        ui.add_enabled(
            state.found.is_some(),
            egui::Checkbox::new(
                &mut settings.mergiraf,
                crate::i18n::text("interaction-settings-mergiraf-offer"),
            ),
        )
        .on_hover_text(crate::i18n::text("interaction-settings-mergiraf-editor"));
    });
    card.row(
        "Git merges",
        "every merge rebase cherry-pick driver",
        |ui| {
            let mut in_merges = state.in_merges;
            let toggle = ui
                .add_enabled(
                    state.found.is_some() && !busy,
                    egui::Checkbox::new(
                        &mut in_merges,
                        crate::i18n::text("interaction-settings-mergiraf-every-merge"),
                    ),
                )
                .on_hover_text(crate::i18n::text("interaction-settings-mergiraf-global"));
            if toggle.changed() {
                mergiraf::use_in_merges(ui.ctx(), in_merges);
            }
        },
    );
}
