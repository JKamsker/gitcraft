//! The profile's transport identity and named forge accounts. Checks run off the UI thread.

use egui::{RichText, Ui};

use super::Settings;
use super::layout::{self, Filter, section};
use crate::connections::{self, Account};
use crate::theme::Tokens;

#[derive(Clone, Default)]
struct Check {
    running: bool,
    result: Option<Result<String, String>>,
    metadata_host: Option<String>,
}

impl Settings {
    pub(super) fn profile_connections_ui(&mut self, ui: &mut Ui, filter: &Filter) {
        let profile = self.profile_mut();
        let id = profile.id;
        section(
            ui,
            filter,
            "Profile's connections",
            Some(crate::i18n::text("interaction-accounts-connections-intro")),
            |card| {
                card.row(
                    "SSH private key",
                    "ssh identity transport core.sshCommand",
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                layout::field(&mut profile.connections.ssh_key)
                                    .hint_text(crate::i18n::text("interaction-accounts-ssh-config"))
                                    .desired_width(layout::fit(ui, 280.0)),
                            );
                            if ui
                                .button(crate::i18n::text("interaction-accounts-browse"))
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_file()
                            {
                                profile.connections.ssh_key = path.to_string_lossy().into_owned();
                            }
                        });
                        note(ui, crate::i18n::text("interaction-accounts-ssh-policy"));
                    },
                );
                for (tool, label, account) in [
                    ("gh", "GitHub", &mut profile.connections.github),
                    ("glab", "GitLab", &mut profile.connections.gitlab),
                ] {
                    card.row(
                        &format!("{label} account"),
                        "forge gh glab username account profile identity",
                        |ui| {
                            account_fields(ui, id, tool, account);
                        },
                    );
                }
                for (service, account) in [
                    (
                        crate::forge::cloud::Service::Azure,
                        &mut profile.connections.azure,
                    ),
                    (
                        crate::forge::cloud::Service::Bitbucket,
                        &mut profile.connections.bitbucket,
                    ),
                ] {
                    card.row(service.name(), "azure bitbucket provider account", |ui| {
                        crate::forge::cloud::ui::show(ui, id, service, account)
                    });
                }
                for (kind, account) in [
                    (crate::trackers::Kind::Jira, &mut profile.connections.jira),
                    (
                        crate::trackers::Kind::Linear,
                        &mut profile.connections.linear,
                    ),
                ] {
                    card.row(
                        kind.name(),
                        "jira linear tracker issue token account",
                        |ui| crate::trackers::ui::account(ui, id, kind, account),
                    );
                }
                card.row(
                    "Forgejo / Gitea login",
                    "tea forge account login alias",
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                layout::field(&mut profile.connections.tea_login)
                                    .hint_text(crate::i18n::text("interaction-accounts-tea-usual"))
                                    .desired_width(layout::fit(ui, 220.0)),
                            );
                            if let Some(host) = account_check(
                                ui,
                                id,
                                "tea",
                                &Account {
                                    user: profile.connections.tea_login.clone(),
                                    ..Default::default()
                                },
                            ) {
                                profile.connections.tea_host = host;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::text("interaction-accounts-https-server"));
                            ui.add(
                                layout::field(&mut profile.connections.tea_host)
                                    .hint_text("codeberg.org")
                                    .desired_width(layout::fit(ui, 220.0)),
                            );
                        });
                        note(ui, crate::i18n::text("interaction-accounts-tea-policy"));
                    },
                );
            },
        );
    }
}

fn note(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .small()
            .color(Tokens::get(ui.ctx()).text_faint),
    );
}

fn account_fields(ui: &mut Ui, profile: u64, tool: &'static str, account: &mut Account) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            layout::field(&mut account.user)
                .hint_text(crate::i18n::text("interaction-accounts-cli-usual"))
                .desired_width(layout::fit(ui, 180.0)),
        );
        account_check(ui, profile, tool, account);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text("interaction-accounts-server"));
        ui.add(
            layout::field(&mut account.host)
                .hint_text(if tool == "gh" {
                    "github.com"
                } else {
                    "gitlab.com"
                })
                .desired_width(layout::fit(ui, 220.0)),
        );
    });
    egui::CollapsingHeader::new(crate::i18n::text("interaction-accounts-config-folder"))
        .id_salt((profile, tool))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    layout::field(&mut account.config_dir)
                        .hint_text(crate::i18n::text("interaction-accounts-normal-folder"))
                        .desired_width(layout::fit(ui, 260.0)),
                );
                if ui
                    .button(crate::i18n::text("interaction-accounts-browse"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    account.config_dir = path.to_string_lossy().into_owned();
                }
            });
            note(
                ui,
                if tool == "gh" {
                    crate::i18n::text("interaction-accounts-gh-policy")
                } else {
                    crate::i18n::text("interaction-accounts-glab-policy")
                },
            );
            if !account.config_dir.is_empty()
                && ui
                    .button(crate::i18n::text("interaction-accounts-copy-login"))
                    .clicked()
            {
                ui.ctx()
                    .copy_text(connections::sign_in_command(tool, account));
            }
        });
}

fn account_check(
    ui: &mut Ui,
    profile: u64,
    tool: &'static str,
    account: &Account,
) -> Option<String> {
    let id = egui::Id::new(("profile-account-check", profile, tool, account.clone()));
    let state = ui
        .ctx()
        .data(|d| d.get_temp::<Check>(id))
        .unwrap_or_default();
    if ui
        .add_enabled(
            account.selected() && !state.running,
            egui::Button::new(if state.running {
                crate::i18n::text("interaction-accounts-checking")
            } else {
                crate::i18n::text("interaction-accounts-check-account")
            }),
        )
        .clicked()
    {
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                id,
                Check {
                    running: true,
                    result: None,
                    metadata_host: None,
                },
            )
        });
        let account = account.clone();
        let ctx = ui.ctx().clone();
        let failed_ctx = ctx.clone();
        let submitted = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                crate::tasks::Cancellation::default(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let result = connections::check_account(tool, &account);
                    let metadata_host = if tool == "tea" && result.is_ok() {
                        crate::forge::selected_tea_metadata(&account.user).map(|(host, _)| host)
                    } else {
                        None
                    };
                    ctx.data_mut(|d| {
                        d.insert_temp(
                            id,
                            Check {
                                running: false,
                                result: Some(result),
                                metadata_host,
                            },
                        )
                    });
                    ctx.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        if let Err(error) = submitted {
            failed_ctx.data_mut(|d| {
                d.insert_temp(
                    id,
                    Check {
                        running: false,
                        result: Some(Err(error)),
                        metadata_host: None,
                    },
                )
            });
        }
    }
    let host = state.metadata_host;
    if host.is_some() {
        ui.ctx().data_mut(|d| {
            d.get_temp_mut_or_default::<Check>(id).metadata_host = None;
        });
    }
    if let Some(result) = state.result {
        ui.label(
            RichText::new(match result {
                Ok(value) => value,
                Err(error) => crate::git::display_error(&error),
            })
            .small(),
        );
    }
    host
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn recorded_account_error_is_translated_only_at_actual_paint() -> Result<(), String> {
        let ui_errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let ui_errors_0 = ui_errors.clone();

        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, account: &mut Account| {
                let ui_result = (|| -> Result<(), String> {
                    if !fonts.replace(true) {
                        crate::theme::install_fonts(ui.ctx());
                        return Ok(());
                    }
                    let id =
                        egui::Id::new(("profile-account-check", 17_u64, "glab", account.clone()));
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            id,
                            Check {
                                result: Some(Err("Account check cancelled.".into())),
                                ..Check::default()
                            },
                        )
                    });
                    account_check(ui, 17, "glab", account);
                    let raw = ui
                        .ctx()
                        .data(|d| d.get_temp::<Check>(id))
                        .ok_or("missing test value")?;
                    assert_eq!(raw.result, Some(Err("Account check cancelled.".into())));
                    assert!(!raw.running);

                    Ok(())
                })();
                if let Err(error) = ui_result {
                    ui_errors_0.borrow_mut().push(error);
                }
            },
            Account::default(),
        );
        h.run();
        h.get_by_label("Kontoprüfung abgebrochen.");
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Account check cancelled.");

        if let Some(error) = ui_errors.borrow().first() {
            return Err(error.clone());
        }

        Ok(())
    }

    #[test]
    fn inert_account_fields_translate_and_keep_account_and_cli_identity() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, account: &mut Account| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                account_fields(ui, 17, "glab", account);
            },
            Account::on("gitlab.com"),
        );
        h.run();
        h.get_by_label("Konto prüfen");
        h.get_by_label("Konfigurationsordner des Kontos (optional)")
            .click();
        h.run();
        h.get_by_label("glab speichert in einem Konfigurationsordner ein Konto je Server. Wähle für ein zweites Konto auf demselben Server einen eigenen Ordner und melde dich mit entsprechend gesetztem GLAB_CONFIG_DIR an. gitcraft prüft den Benutzernamen vor dem Senden von Änderungen.");
        assert_eq!(h.state(), &Account::on("gitlab.com"));
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Check account");
        h.get_by_label("Account configuration folder (optional)");
        assert_eq!(h.state(), &Account::on("gitlab.com"));
    }
}
