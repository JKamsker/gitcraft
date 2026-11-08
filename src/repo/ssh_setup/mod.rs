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
            status: crate::i18n::text("repo-workflows-ssh_setup-new-8633c172").into(),
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
    fn background(
        &mut self,
        ctx: &egui::Context,
        job: impl FnOnce(&dyn Fn() -> bool) -> Result<Ready, String> + Send + 'static,
    ) {
        self.cancel_worker();
        let (send, receive) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        self.active = Some(active.clone());
        self.receiver = Some(receive);
        self.status = "Working on the selected machine…".into();
        let ctx = ctx.clone();
        let repo = self.repo.clone();
        let failed_ctx = ctx.clone();
        let result = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                self.jobs.0.child(),
                crate::tasks::Priority::Interactive,
                Some(repo.clone()),
                Box::new(move || {
                    let live = || active.load(Ordering::Acquire);
                    let result = if crate::practice::is_repository(&repo) {
                        Err(
                            crate::i18n::text("repo-workflows-ssh_setup-background-eeca6d22")
                                .into(),
                        )
                    } else {
                        job(&live)
                    };
                    if live() {
                        let _ = send.send(result);
                    }
                    ctx.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        if let Err(error) = result {
            self.receiver = None;
            self.status = error;
            failed_ctx.request_repaint();
        }
    }
    fn launch(
        &mut self,
        invocation: Invocation,
        step: Step,
        key: Option<String>,
        ctx: &egui::Context,
    ) {
        // Folder reservation and all auth/filesystem work have already happened off the UI
        // thread. This uses the same PTY constructor as the ordinary integrated terminal.
        match Terminal::spawn(&std::env::temp_dir(),&invocation.shell,ctx) {
            Ok(terminal)=>{
                if let Some(key)=key {self.key=key;}
                self.command=Some(CommandRun {terminal,step,finished:false});
                self.status=format!("{} is running. Enter any passphrase only in the terminal below.",step.label());
            }
            Err(_)=>self.status="Could not start the selected machine's terminal/tool. Check that OpenSSH/WSL is installed. A reserved new folder, if any, is retained; existing files were not overwritten.".into(),
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if connections::current() != self.context
            || Settings::get(ctx).active_profile != self.profile_id
        {
            self.cancel_worker();
            self.account = None;
            self.command = None;
            self.context = connections::current();
            self.profile_id = Settings::get(ctx).active_profile;
            self.status="The active profile or its connection choices changed. Check the intended account again before uploading.".into();
        }
        let ready = self
            .receiver
            .as_ref()
            .and_then(|receive| match receive.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(Err(crate::i18n::text(
                    "repo-workflows-ssh_setup-poll-380743c6",
                )
                .into())),
            });
        if let Some(ready) = ready {
            self.cancel_worker();
            match ready {
                Err(error)=>self.status=error,
                Ok(Ready::Command{invocation,step,key})=>self.launch(invocation,step,key,ctx),
                Ok(Ready::Account(account))=>{self.status=format!("Checked account: {} on {}. Upload is a separate action.",account.owner(),self.host);self.account=Some(account);}
                Ok(Ready::Uploaded)=>self.status="The selected public key was registered. This does not enable commit signing or change a global account.".into(),
                Ok(Ready::Adopt(path))=>{
                    let mut settings=Settings::get(ctx);
                    if settings.active_profile==self.profile_id {
                        settings.profile_mut().connections.ssh_key=path.to_string_lossy().into_owned();
                        settings.set(ctx);self.context=connections::current();
                        self.status="This profile now selects that local SSH key. Author/signing and repository-local SSH overrides retain their existing precedence.".into();
                    }
                }
            }
        }
        if let Some(run) = &mut self.command
            && !run.finished
        {
            if let Some(code) = run.terminal.exit_code() {
                run.finished = true;
                self.status = if run.step == Step::Test {
                    format!(
                        "SSH test exited {code}. Read the server greeting below: forges often return1 after successful authentication because shell access is disabled."
                    )
                } else if code == 0 {
                    format!(
                        "{} command completed successfully. Key files are retained; continue with the next explicit step.",
                        run.step.label()
                    )
                } else {
                    format!(
                        "{} exited {code}. Read the terminal's explanation. No existing key was overwritten by generation; missing agent/tool/permission must be fixed on this machine.",
                        run.step.label()
                    )
                };
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
    fn prepare_command(&mut self, ctx: &egui::Context, step: Step) {
        let machine = self.machine.clone();
        let key = self.key.clone();
        let folder = self.folder.clone();
        let title = self.title.clone();
        let destination = self.destination.clone();
        let host = self.host.clone();
        let account = self.account_name.clone();
        let forge = self.forge;
        self.background(ctx, move |live| {
            let (invocation, new_key) = match step {
                Step::Generate => {
                    let (invocation, key) = plan::generate(&machine, &folder, &title)?;
                    (invocation, Some(key))
                }
                Step::Agent => (plan::agent(&machine, &key)?, None),
                Step::Test => (plan::test(&machine, &key, &destination)?, None),
                Step::Register => (
                    plan::remote_registration(
                        &machine,
                        &key,
                        &host,
                        (forge == Forge::GitHub).then_some(account.as_str()),
                        (forge != Forge::GitHub).then_some(account.as_str()),
                        &title,
                    )?,
                    None,
                ),
            };
            if !live() {
                return Err(
                    crate::i18n::text("repo-workflows-ssh_setup-prepare-command-31585a34").into(),
                );
            }
            if let Some(folder) = &invocation.reserve_folder {
                reserve_folder(folder)?;
            }
            if !live() {
                return Err(
                    crate::i18n::text("repo-workflows-ssh_setup-prepare-command-78845053").into(),
                );
            }
            Ok(Ready::Command {
                invocation,
                step,
                key: new_key,
            })
        });
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
    fn fields(&mut self, ui: &mut Ui) {
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-ssh_setup-fields-4405a792",
                &[(
                    "self-machine-label-1",
                    (self.machine.label()).clone().into(),
                )],
            ))
            .strong(),
        );
        if !self.machine.is_local() {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-69608cb8",
            ));
        }
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-a7b7ae57",
        ));
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-08b5218c",
            ));
            ui.text_edit_singleline(&mut self.folder);
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-25f08261",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-e67b66d2",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Generate);
        }
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-a8ff2446",
            ));
            ui.text_edit_singleline(&mut self.key);
            if self.machine.is_local()
                && ui
                    .button(crate::i18n::text(
                        "repo-workflows-ssh_setup-fields-42ad2d5a",
                    ))
                    .clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                self.key = path.to_string_lossy().into_owned();
            }
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-a6faa7c0",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-1cb4035b",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Agent);
        }
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-75c9835b",
        ));
        ui.separator();
        let old = (self.forge, self.host.clone(), self.account_name.clone());
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("ssh_setup_forge")
                .selected_text(self.forge.name())
                .show_ui(ui, |ui| {
                    for forge in [Forge::GitHub, Forge::Forgejo, Forge::Gitea] {
                        ui.selectable_value(&mut self.forge, forge, forge.name());
                    }
                });
            if self.machine.is_local() || self.forge == Forge::GitHub {
                ui.label(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-467c73ec",
                ));
                ui.text_edit_singleline(&mut self.host);
            } else {
                ui.label(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-2fc162ec",
                ));
            }
        });
        ui.horizontal(|ui| {
            ui.label(if self.forge == Forge::GitHub {
                crate::i18n::text("repo-workflows-ssh_setup-fields-fa174850")
            } else {
                crate::i18n::text("repo-workflows-ssh_setup-fields-9c3b602e")
            });
            ui.text_edit_singleline(&mut self.account_name);
        });
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-2b81b1a8",
            ));
            ui.text_edit_singleline(&mut self.title);
        });
        if old != (self.forge, self.host.clone(), self.account_name.clone()) {
            self.account = None;
        }
        if self.machine.is_local() {
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-ssh_setup-fields-9487c9a7",
                    ))
                    .clicked()
                {
                    match self.hosting() {
                        Ok(hosting) => {
                            let name = self.account_name.clone();
                            self.background(ui.ctx(), move |live| {
                                ssh_keys::prepare(&hosting, &name, live).map(Ready::Account)
                            });
                        }
                        Err(e) => self.status = e,
                    }
                }
                if ui
                    .add_enabled(
                        self.account.is_some(),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-ssh_setup-fields-71b7ffa7",
                        )),
                    )
                    .clicked()
                    && let Some(account) = self.account.take()
                {
                    let machine = self.machine.clone();
                    let key = self.key.clone();
                    let title = self.title.clone();
                    self.background(ui.ctx(), move |live| {
                        let path = public_companion(&machine, &key)?;
                        let public = ssh_keys::public_key::read(&path)?;
                        account.upload(&title, &public, live)?;
                        Ok(Ready::Uploaded)
                    });
                }
            });
            if let Some(account) = &self.account {
                ui.label(crate::i18n::message(
                    "repo-workflows-ssh_setup-fields-0aaeaa7d",
                    &[
                        ("account-owner-1", (account.owner()).clone().into()),
                        ("self-host-2", (self.host).clone().into()),
                    ],
                ));
            }
        } else {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-c362f511",
            ));
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-005427d8",
                ))
                .clicked()
            {
                self.prepare_command(ui.ctx(), Step::Register);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-76f62ede",
            ));
            ui.text_edit_singleline(&mut self.destination);
        });
        ui.label(crate::i18n::text(
            "repo-workflows-ssh_setup-fields-391d0d15",
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-ssh_setup-fields-4a4bcca6",
            ))
            .clicked()
        {
            self.prepare_command(ui.ctx(), Step::Test);
        }
        if self.machine.is_local()
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-ssh_setup-fields-c62cdf9c",
                ))
                .clicked()
        {
            let path = PathBuf::from(self.key.clone());
            let machine = self.machine.clone();
            let key = self.key.clone();
            self.background(ui.ctx(), move |_| {
                machine.validate_path(&key)?;
                let meta = std::fs::metadata(&path).map_err(|_| {
                    crate::i18n::text("repo-workflows-ssh_setup-fields-fdfcad0a").to_owned()
                })?;
                if !meta.is_file() {
                    return Err(
                        crate::i18n::text("repo-workflows-ssh_setup-fields-c37c192f").into(),
                    );
                }
                Ok(Ready::Adopt(path))
            });
        }
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
            ui.label(crate::repo::messages::display_status(&state.status));
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
