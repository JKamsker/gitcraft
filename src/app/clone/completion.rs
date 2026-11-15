//! Completion follows the initiating viewport and retained clone options.
use super::*;
impl App {
    pub(in crate::app) fn poll_clone(&mut self, ctx: &egui::Context) {
        let Some(cloning) = &self.cloning else {
            return;
        };
        let Ok(result) = cloning.rx.try_recv() else {
            return;
        };
        let dest = cloning.dest.clone();
        let owner = cloning.owner;
        let open_after = cloning.open_after;
        let cancelled = cloning.control.is_cancelled();
        self.cloning = None;
        self.in_window(owner, |app| {
            app.finish_clone(dest, cancelled, open_after, result, ctx)
        });
    }

    fn finish_clone(
        &mut self,
        dest: PathBuf,
        cancelled: bool,
        open_after: bool,
        result: Result<PathBuf, String>,
        ctx: &egui::Context,
    ) {
        match result {
            Err(_) if cancelled => {
                self.clone_failed = false;
                self.error = Some(format!("Clone into {}: cancelled", dest.display()).into());
            }
            Ok(root) if !open_after => {
                self.remember(&root);
            }
            // On another machine, it opens once reached again, the credentials along.
            Ok(_) if crate::remote::is_remote(&dest) => {
                let creds = self
                    .cloning_form
                    .take()
                    .filter(|f| f.auth)
                    .and_then(|form| {
                        git::Credentials::for_url(
                            form.url.trim(),
                            form.username.trim().to_owned(),
                            form.password,
                        )
                        .ok()
                    });
                self.connect(dest, creds, ctx);
            }
            Ok(root) => {
                // Show it in a start page tab if one is active, otherwise in a new tab.
                self.open_root(root.clone(), ctx);
                // The credentials that cloned it also fetch and push in its tab, and in no
                // other: when it couldn't be opened, the active tab is another repository's.
                if let Some(form) = self.cloning_form.take().filter(|f| f.auth)
                    && let Ok(creds) = git::Credentials::for_url(
                        form.url.trim(),
                        form.username.trim().to_owned(),
                        form.password,
                    )
                {
                    self.credentials_for(&root, creds);
                }
            }
            // Back to the form, asking for credentials.
            Err(e) if git::is_auth_error(&e) => match self.cloning_form.take() {
                Some(mut form) => {
                    form.error = Some(if form.auth {
                        "Those credentials were refused.".to_owned()
                    } else {
                        "This repository needs you to sign in.".to_owned()
                    });
                    form.auth = true;
                    form.password.clear();
                    self.clone_form = Some(form);
                }
                None => self.error = Some(format!("Clone failed: {e}").into()),
            },
            Err(e) => {
                self.clone_failed = true;
                self.error = Some(format!("Clone failed: {e}").into());
            }
        }
        self.cloning_form = None;
    }

    #[cfg(test)]
    pub(in crate::app) fn recorded_clone_completion(
        &mut self,
        owner: egui::ViewportId,
        root: PathBuf,
    ) {
        self.start_clone();
        if let Some(form) = &mut self.clone_form {
            form.auth = true;
            form.url = "https://example.invalid/recorded/repository.git".into();
            form.username = "recorded-user".into();
            form.password = "recorded-secret".into();
        }
        self.cloning_form = self.clone_form.take();
        let (tx, rx) = channel();
        let _ = tx.send(Ok(root.clone()));
        self.cloning = Some(Cloning {
            owner,
            open_after: true,
            dest: root,
            rx,
            control: git::OpControl::new(|| {}),
        });
    }
    #[cfg(test)]
    pub(in crate::app) fn recorded_clone_without_open(
        &mut self,
        owner: egui::ViewportId,
        root: PathBuf,
    ) {
        self.recorded_clone_completion(owner, root);
        if let Some(cloning) = &mut self.cloning {
            cloning.open_after = false;
        }
    }
}
