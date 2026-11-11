//! Explicit SSH setup on the repository machine, with existing PTY/account ownership.
mod plan;
#[cfg(test)]
mod tests;

use super::RepoTab;
use crate::connections::{self, Context};
use crate::forge::{
    Forge, Hosting,
    ssh_keys::{self, SshKeyAccount},
};
use crate::settings::Settings;
use crate::terminal::Terminal;
use egui::{RichText, Ui};
use plan::{Invocation, Machine};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Generate,
    Agent,
    Register,
    Test,
}
impl Step {
    fn label(self) -> &'static str {
        match self {
            Self::Generate => "Create key",
            Self::Agent => "Add to agent",
            Self::Register => "Register public key",
            Self::Test => "Test SSH",
        }
    }
}
struct CommandRun {
    terminal: Terminal,
    step: Step,
    finished: bool,
}
enum Ready {
    Command {
        invocation: Invocation,
        step: Step,
        key: Option<String>,
    },
    Account(SshKeyAccount),
    Uploaded,
    Adopt(PathBuf),
}
pub(super) struct State {
    machine: Machine,
    repo: PathBuf,
    context: Context,
    profile_id: u64,
    folder: String,
    key: String,
    title: String,
    forge: Forge,
    host: String,
    account_name: String,
    destination: String,
    account: Option<SshKeyAccount>,
    receiver: Option<Receiver<Result<Ready, String>>>,
    active: Option<Arc<AtomicBool>>,
    command: Option<CommandRun>,
    status: String,
    jobs: crate::tasks::Scope,
}
impl State {
    fn new(repo: &RepoTab, ctx: &egui::Context) -> Result<Self, String> {
        let machine = Machine::of(&repo.path)?;
        let context = connections::current();
        let settings = Settings::get(ctx);
        let hosting = repo.hosting.as_ref();
        let forge = hosting.map_or(Forge::GitHub, |h| {
            if h.forge == Forge::GitLab {
                Forge::GitHub
            } else {
                h.forge
            }
        });
        let host = hosting
            .filter(|h| h.forge != Forge::GitLab)
            .map_or("github.com", |h| {
                h.web.strip_prefix("https://").unwrap_or(&h.host)
            })
            .to_owned();
        let account_name = if machine.is_local() {
            if forge == Forge::GitHub {
                context.choices.github.user.clone()
            } else {
                context.choices.tea_login.clone()
            }
        } else {
            String::new()
        };
        let home = if machine.is_local() {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
        } else {
            None
        };
        let folder = home
            .map(|h| h.join("gitcraft-ssh-key").to_string_lossy().into_owned())
            .unwrap_or_else(|| "~/gitcraft-ssh-key".into());
        let key = if machine.is_local() {
            context.choices.ssh_key.clone()
        } else {
            String::new()
        };
        Ok(Self {
            repo: repo.path.clone(),
            machine,
            context,
            profile_id: settings.active_profile,
            folder,
            key,
            title: "gitcraft".into(),
            forge,
            host: host.clone(),
            account_name,
            destination: format!(
                "git@{}",
                hosting
                    .filter(|h| h.forge != Forge::GitLab)
                    .map_or("github.com", |h| h.host.as_str())
            ),
            account: None,
            receiver: None,
            active: None,
            command: None,
            jobs: crate::tasks::Scope(repo.tx.cancel.child()),
            status:
                "No key, agent, account or network action has run. Choose each step explicitly."
                    .into(),
        })
    }
    pub fn running(&self) -> bool {
        self.receiver.is_some() || self.command.is_some()
    }
    fn busy(&mut self) -> bool {
        self.receiver.is_some()
            || self
                .command
                .as_mut()
                .is_some_and(|run| run.terminal.exit_code().is_none())
    }
    fn cancel_worker(&mut self) {
        if let Some(active) = self.active.take() {
            active.store(false, Ordering::Release);
        }
        self.receiver = None;
        self.jobs.0.cancel();
        self.jobs = crate::tasks::Scope::default();
    }

    fn hosting(&self) -> Result<Hosting, String> {
        connections::Account {
            host: self.host.clone(),
            user: "check".into(),
            config_dir: String::new(),
        }
        .validate()?;
        Ok(Hosting {
            forge: self.forge,
            host: self.host.to_ascii_lowercase(),
            path: String::new(),
            web: format!("https://{}", self.host.to_ascii_lowercase()),
        })
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel_worker();
    }
}

fn public_companion(machine: &Machine, key: &str) -> Result<PathBuf, String> {
    machine.validate_path(key)?;
    if !machine.is_local() {
        return Err(crate::i18n::text("repo-workflows-ssh_setup-public-companion-b3c70a23").into());
    }
    Ok(PathBuf::from(format!("{key}.pub")))
}

fn reserve_folder(folder: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    let create = {
        use std::os::unix::fs::DirBuilderExt;
        let mut create = std::fs::DirBuilder::new();
        create.mode(0o700);
        create
    };
    #[cfg(not(unix))]
    let create = std::fs::DirBuilder::new();
    create
        .create(folder)
        .map_err(|_| crate::i18n::text("repo-workflows-ssh_setup-reserve-folder-2ab0aef7").into())
}

impl RepoTab {
    pub(super) fn open_ssh_setup(&mut self, ctx: &egui::Context) {
        if self.practice.is_some() {
            self.status=Some(crate::repo::Notice::plain(("SSH setup is unavailable in Practice repositories; open a real repository to choose account/key actions.".into(),true)));
            return;
        }
        match State::new(self, ctx) {
            Ok(state) => self.ssh_setup = Some(state),
            Err(error) => self.status = Some(crate::repo::Notice::plain((error, true))),
        }
    }
    pub(super) fn ssh_setup_window(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.ssh_setup.take() else {
            return;
        };
        state.poll(ctx);
        let busy = state.busy();
        let mut open = true;
        let height = ctx.content_rect().height();
        let terminal_height = (height * 0.3).clamp(100.0, 260.0);
        let reserved = if state.command.is_some() {
            terminal_height + 180.0
        } else {
            150.0
        };
        let fields_height = (height - reserved).clamp(60.0, 430.0);
        egui::Window::new(crate::i18n::text(
            "repo-workflows-ssh_setup-ssh-setup-window-74073cb8",
        ))
        .id(egui::Id::new(("ssh_setup", &self.path)))
        .open(&mut open)
        .default_width(760.0)
        .max_height((height - 16.0).max(100.0))
        .vscroll(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("ssh_setup_fields")
                .max_height(fields_height)
                .show(ui, |ui| ui.add_enabled_ui(!busy, |ui| state.fields(ui)));
            ui.separator();
            ui.label(crate::repo::messages::display_status(
                &crate::repo::activity::display_workflow_error(&state.status),
            ));
            if busy {
                ui.label(crate::i18n::text(
                    "repo-workflows-ssh_setup-ssh-setup-window-72a17b9c",
                ));
            }
            if let Some(run) = &mut state.command {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), terminal_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| run.terminal.ui(ui),
                );
            }
        });

        if open {
            self.ssh_setup = Some(state);
        }
    }
}

#[cfg(test)]
mod fluent_r1_status_tests {
    #[test]
    fn fluent_partition_r1_ssh_fixed_cached_status_follows_locale_without_account_or_command_replay()
     {
        for raw in [
            "No key, agent, account or network action has run. Choose each step explicitly.",
            "Setup worker stopped; no later result will be applied.",
        ] {
            let retained = raw.to_owned();
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                assert_eq!(
                    crate::repo::activity::display_workflow_error(&retained) == retained,
                    lang == "en-US"
                );
                assert_eq!(retained, raw);
            }
        }
        crate::i18n::set_language("en-US");
    }
}

mod fields;

mod worker;
