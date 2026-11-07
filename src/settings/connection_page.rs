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
            Some(
                "Choose the identity used to push and review. Empty account names keep the CLI's usual account. These choices affect gitcraft's commands, without switching any CLI's global active account. Git in WSL or over SSH keeps that machine's keys and credentials; forge requests still run here with this profile's account.",
            ),
            |card| {
                card.row("SSH private key", "ssh identity transport core.sshCommand", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(layout::field(&mut profile.connections.ssh_key)
                            .hint_text("As in your SSH config")
                            .desired_width(layout::fit(ui, 280.0)));
                        if ui.button("Browse…").clicked()
                            && let Some(path) = rfd::FileDialog::new().pick_file()
                        {
                            profile.connections.ssh_key = path.to_string_lossy().into_owned();
                        }
                    });
                    note(ui, "An absolute path on this computer, separate from the commit-signing key. Connections authenticate directly with this key file; shared SSH masters, agent identities and certificates are not substituted. Encrypted keys can ask for their passphrase. Other IdentityFile/CertificateFile entries for the host are refused. A repository's own core.sshCommand still wins.");
                });
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
                card.row("Forgejo / Gitea login", "tea forge account login alias", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.add(layout::field(&mut profile.connections.tea_login)
                            .hint_text("tea's usual login for this server")
                            .desired_width(layout::fit(ui, 220.0)));
                        if let Some(host)=account_check(ui, id, "tea", &Account {
                            user: profile.connections.tea_login.clone(),
                            ..Default::default()
                        }) { profile.connections.tea_host=host; }
                    });
                    ui.horizontal(|ui| {
                        ui.label("HTTPS server");
                        ui.add(layout::field(&mut profile.connections.tea_host).hint_text("codeberg.org").desired_width(layout::fit(ui,220.0)));
                    });
                    note(ui, "The login name from tea login add --name …. A selected login is used only on its own HTTPS server, without falling back to another account.");
                });
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
                .hint_text("CLI's usual account")
                .desired_width(layout::fit(ui, 180.0)),
        );
        account_check(ui, profile, tool, account);
    });
    ui.horizontal(|ui| {
        ui.label("Server");
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
    egui::CollapsingHeader::new("Account configuration folder (optional)")
        .id_salt((profile, tool))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(layout::field(&mut account.config_dir)
                    .hint_text("CLI's normal configuration folder")
                    .desired_width(layout::fit(ui, 260.0)));
                if ui.button("Browse…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    account.config_dir = path.to_string_lossy().into_owned();
                }
            });
            note(ui, if tool == "gh" {
                "gh selects this named signed-in account directly. A separate folder is optional."
            } else {
                "glab stores one account per server in a configuration folder. For a second account on the same server, choose its own folder and sign in with GLAB_CONFIG_DIR set to it. gitcraft verifies the username before sending changes."
            });
            if !account.config_dir.is_empty() && ui.button("Copy sign-in command").clicked() {
                ui.ctx().copy_text(connections::sign_in_command(tool, account));
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
                "Checking…"
            } else {
                "Check account"
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
                Err(error) => error,
            })
            .small(),
        );
    }
    host
}
