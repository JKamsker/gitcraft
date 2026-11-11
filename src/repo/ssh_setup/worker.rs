//! Worker lifecycle, terminal completion, and command preparation.

use super::*;

impl State {
    pub(super) fn background(
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

    pub(super) fn launch(
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

    pub(super) fn poll(&mut self, ctx: &egui::Context) {
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
                Err(_) => Some(Err(
                    "Setup worker stopped; no later result will be applied.".into(),
                )),
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

    pub(super) fn prepare_command(&mut self, ctx: &egui::Context, step: Step) {
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
}
