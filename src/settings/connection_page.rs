//! The profile's transport identity and named forge accounts. Checks run off the UI thread.

use egui::{RichText, Ui};

use super::Settings;
use super::layout::{self, Filter, section};
use crate::connections::{self, Account, CloudAccount};
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
                    note(ui, "An absolute path on this computer, separate from the commit-signing key. Other IdentityFile entries for the host are refused to avoid using another account. A repository's own core.sshCommand still wins.");
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
                for (service, account) in [
                    (crate::forge::CloudService::Azure, &mut profile.connections.azure),
                    (crate::forge::CloudService::Bitbucket, &mut profile.connections.bitbucket),
                ] {
                    card.row(&format!("{} account", service.name()), "azure bitbucket provider account identity", |ui| {
                        cloud_account_fields(ui, id, service, account);
                    });
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
        connections::spawn(move || {
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
        });
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

#[derive(Clone, Default)]
struct CloudCheck {
    running: bool,
    result: Option<Result<CloudAccount, String>>,
}
fn cloud_source(account: &CloudAccount) -> CloudAccount {
    let mut source=account.clone();
    source.subject.clear();
    source
}
fn cloud_account_fields(ui:&mut Ui, profile:u64, service:crate::forge::CloudService, account:&mut CloudAccount) {
    let mut changed=false;
    ui.horizontal_wrapped(|ui| {
        ui.label("Expected account name or ID");
        changed|=ui.add(layout::field(&mut account.user).hint_text("Public account name, UUID or Azure unique name").desired_width(layout::fit(ui,250.0))).changed();
    });
    ui.horizontal(|ui| {
        ui.label(if service==crate::forge::CloudService::Azure {"Organization HTTPS URL"} else {"Service"});
        changed|=ui.add(layout::field(&mut account.server).hint_text(if service==crate::forge::CloudService::Azure {"https://dev.azure.com/organization"}else{"https://bitbucket.org"}).desired_width(layout::fit(ui,300.0))).changed();
    });
    match service {
        crate::forge::CloudService::Azure=>{
            changed|=ui.add(layout::field(&mut account.config_dir).hint_text("Optional local AZURE_CONFIG_DIR").desired_width(layout::fit(ui,300.0))).changed();
            note(ui,"Install Azure CLI and sign in locally. gitcraft verifies the Azure DevOps server's authenticated identity before repository requests; it never stores the access token.");
        }
        crate::forge::CloudService::Bitbucket=>{
            changed|=ui.add(layout::field(&mut account.credential_user).hint_text("Credential-store username / Atlassian email (optional)").desired_width(layout::fit(ui,300.0))).changed();
            note(ui,"Use your local Git credential store for https://bitbucket.org. Enter its public username or Atlassian email here, never a password or API token. The API account ID is verified before repository requests.");
        }
    }
    if changed {account.subject.clear();}
    let source=cloud_source(account);
    let key=egui::Id::new(("cloud-account-check",profile,service,source.clone()));
    let state=ui.ctx().data(|d|d.get_temp::<CloudCheck>(key)).unwrap_or_default();
    if let Some(result)=state.result {
        match result {
            Ok(verified) if cloud_source(&verified)==source=>{
                account.subject=verified.subject;
                note(ui,&format!("Verified public owner: {}",account.subject));
            }
            Ok(_)=>{},
            Err(error)=>note(ui,&error),
        }
    }
    if ui.add_enabled(account.selected()&&!state.running,egui::Button::new(if state.running {"Checking provider…"}else{"Check provider account"})).clicked() {
        ui.ctx().data_mut(|d|d.insert_temp(key,CloudCheck{running:true,result:None}));
        let account=account.clone();let ctx=ui.ctx().clone();
        connections::spawn(move||{
            let result=crate::forge::verify_cloud_account(service,&account);
            ctx.data_mut(|d|d.insert_temp(key,CloudCheck{running:false,result:Some(result)}));
            ctx.request_repaint();
        });
    }
    note(ui,"These selections authenticate forge API actions. Git transport still uses the profile's existing SSH/Git credential configuration; portable clones never fall back to an ambient personal login.");
}
