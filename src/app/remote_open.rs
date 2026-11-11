//! Opening a repository on another machine (WSL, SSH): the dialog, and the connection made in
//! the background (the first one to an SSH host takes a second or more).

mod fields;
pub(in crate::app) use fields::*;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Stroke, Ui, vec2};

use super::panels::Toast;
use super::{App, Tab, repo_root};
use crate::folder_browser::{FolderBrowser, Outcome};
use crate::git;
use crate::remote::{self, Host};
use crate::theme::{self, Tokens};

/// Which kind of machine the dialog opens a repository on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Wsl,
    Ssh,
}

pub(super) struct RemoteForm {
    kind: Kind,
    /// The WSL distributions installed, and the one picked.
    distros: Vec<String>,
    distro: String,
    /// The distributions, while `wsl.exe` lists them (it can take a while to start).
    distros_rx: Option<Receiver<Vec<String>>>,
    /// The hosts of `~/.ssh/config`, and the host typed or picked.
    hosts: Vec<String>,
    host: String,
    path: String,
    error: Option<String>,
    focus: bool,
    /// The folders of the machine, browsed for the folder (in place of the form while open).
    browser: Option<FolderBrowser>,
}

/// A repository on another machine being reached: where, and what came of it.
pub(super) struct Connecting {
    owner: egui::ViewportId,
    /// Where, as the start page says it.
    pub(super) label: String,
    rx: Receiver<Result<PathBuf, super::AppError>>,
    /// Credentials typed to clone it, for its tab.
    creds: Option<git::Credentials>,
}

impl RemoteForm {
    fn new(ctx: &egui::Context) -> Self {
        let hosts = remote::ssh_config_hosts();
        let distros_rx = cfg!(windows).then(|| {
            let (tx, rx) = channel();
            let ctx = ctx.clone();
            crate::connections::spawn(move || {
                let _ = tx.send(remote::wsl_distros());
                ctx.request_repaint();
            });
            rx
        });
        let kind = if distros_rx.is_some() {
            Kind::Wsl
        } else {
            Kind::Ssh
        };
        Self {
            kind,
            distro: String::new(),
            distros: Vec::new(),
            distros_rx,
            host: hosts.first().cloned().unwrap_or_default(),
            hosts,
            path: String::new(),
            error: None,
            focus: true,
            browser: None,
        }
    }

    /// The distributions, once listed: the first one picked; SSH when there are none.
    fn take_distros(&mut self) {
        let Some(distros) = self.distros_rx.as_ref().and_then(|rx| rx.try_recv().ok()) else {
            return;
        };
        self.distros_rx = None;
        if distros.is_empty() && self.kind == Kind::Wsl {
            self.kind = Kind::Ssh;
        }
        self.distro = distros.first().cloned().unwrap_or_default();
        self.distros = distros;
    }

    /// The machine and the folder there the form names, or why it can't be opened.
    fn target(&self) -> Result<(Host, String), String> {
        let path = self.path.trim();
        if !(path.starts_with('/') || path == "~" || path.starts_with("~/")) {
            return Err(
                "The folder is a path on that machine: /home/me/repo or ~/repo.".to_owned(),
            );
        }
        Ok((self.host()?, path.to_owned()))
    }

    /// The machine the form names, or why it names none.
    fn host(&self) -> Result<Host, String> {
        Ok(match self.kind {
            Kind::Wsl => {
                let distro = self.distro.trim();
                if !remote::valid_distro(distro) {
                    return Err("Pick a WSL distribution.".to_owned());
                }
                Host::Wsl(distro.to_owned())
            }
            Kind::Ssh => {
                let host = self.host.trim();
                if !remote::valid_ssh_host(host) {
                    return Err(
                        "The host is what ssh takes: an alias of ~/.ssh/config or user@host."
                            .to_owned(),
                    );
                }
                Host::Ssh(host.to_owned())
            }
        })
    }

    /// Opens the folder browser on the machine named, at the folder typed (else its home).
    fn browse(&mut self, ctx: &egui::Context) {
        match self.host() {
            Ok(host) => {
                self.error = None;
                self.browser = Some(FolderBrowser::new(host, &self.path, ctx));
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Draws the folder browser; once a folder is picked (or the browser left), the form again.
    /// Whether it is open.
    fn browser_ui(&mut self, ui: &mut Ui) -> bool {
        let Some(browser) = &mut self.browser else {
            return false;
        };
        match browser.ui(ui) {
            Some(Outcome::Picked(path)) => {
                self.path = path;
                self.error = None;
                self.browser = None;
            }
            Some(Outcome::Cancelled) => self.browser = None,
            None => {}
        }
        true
    }
}

impl App {
    #[cfg(test)]
    pub(super) fn recorded_connection_completion(
        &mut self,
        owner: egui::ViewportId,
        root: PathBuf,
    ) {
        let (tx, rx) = channel();
        let _ = tx.send(Ok(root));
        self.connecting.push(Connecting {
            owner,
            label: "recorded".into(),
            rx,
            creds: git::Credentials::for_url(
                "https://example.invalid/recorded/repository.git",
                "recorded-user".into(),
                "recorded-secret".into(),
            )
            .ok(),
        });
    }

    pub(super) fn start_open_remote(&mut self, ctx: &egui::Context) {
        self.remote_form = Some(RemoteForm::new(ctx));
    }

    /// Reaches the repository at `path` on another machine in the background; its tab opens
    /// once it answers. `creds`: what cloned it, for that tab to fetch and push with.
    pub(super) fn connect(
        &mut self,
        path: PathBuf,
        creds: Option<git::Credentials>,
        ctx: &egui::Context,
    ) {
        let label = path.display().to_string();
        self.connect_with(label, creds, ctx, move || {
            repo_root(&path).map_err(Into::into)
        });
    }

    /// Opens, once `find` (run in the background) gives its top folder, a repository on
    /// another machine; `label` says where meanwhile.
    pub(super) fn connect_with(
        &mut self,
        label: String,
        creds: Option<git::Credentials>,
        ctx: &egui::Context,
        find: impl FnOnce() -> Result<PathBuf, super::AppError> + Send + 'static,
    ) {
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        crate::connections::spawn(move || {
            // Said in the window, even if finding it fails unexpectedly.
            let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(find))
                .unwrap_or_else(|_| Err("Connecting failed unexpectedly.".to_owned().into()));
            let _ = tx.send(found);
            ctx2.request_repaint();
        });
        self.error = None;
        // The start page says so; any other tab, a toast.
        if !matches!(self.tabs.get(self.active), Some(Tab::Home)) {
            self.toasts
                .push(Toast::new(format!("Connecting to {label}…"), false));
        }
        self.connecting.push(Connecting {
            owner: self.windows.current,
            label,
            rx,
            creds,
        });
    }

    /// Opens the repositories reached since, each in its tab; says why one couldn't be.
    pub(super) fn poll_connecting(&mut self, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.connecting.retain(|c| match c.rx.try_recv() {
            Ok(result) => {
                done.push((c.owner, result, c.creds.clone()));
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        for (owner, result, creds) in done {
            self.in_window(owner, |app| app.finish_connection(result, creds, ctx));
        }
    }

    pub(super) fn finish_connection(
        &mut self,
        result: Result<PathBuf, super::AppError>,
        creds: Option<git::Credentials>,
        ctx: &egui::Context,
    ) {
        match result {
            Ok(root) => {
                self.open_root(root.clone(), ctx);
                if let Some(creds) = creds {
                    self.credentials_for(&root, creds);
                }
            }
            Err(e) => self.report(e),
        }
    }

    pub(super) fn remote_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.remote_form else {
            return;
        };
        form.take_distros();
        // ssh asking something (in its own dialog, over this one) has the keys.
        let asked = crate::remote::askpass::pending().is_some();
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut submit = false;
        let mut cancel = false;
        let resp = egui::Modal::new(egui::Id::new("remote-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(
                    RichText::new(crate::i18n::text("interaction-forms-remote-title"))
                        .font(theme::semibold(15.0)),
                );
                if form.browser.is_some() {
                    ui.add_space(8.0);
                    form.browser_ui(ui);
                    return;
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(crate::i18n::text("interaction-forms-remote-explanation"))
                        .color(t.text_dim)
                        .small(),
                );
                ui.add_space(10.0);
                let (browse, enter) = form_fields(ui, form, &t);
                if browse {
                    form.browse(ui.ctx());
                }
                if let Some(err) = &form.error {
                    ui.add_space(4.0);
                    ui.colored_label(
                        t.danger,
                        display_message(err).unwrap_or_else(|| git::display_error(err)),
                    );
                }
                ui.add_space(12.0);
                let valid = !form.path.trim().is_empty()
                    && match form.kind {
                        Kind::Wsl => !form.distro.trim().is_empty(),
                        Kind::Ssh => !form.host.trim().is_empty(),
                    };
                ui.horizontal(|ui| {
                    let enter = !asked && !browse && enter;
                    if crate::repo::primary_button(
                        ui,
                        crate::i18n::text("interaction-forms-open"),
                        valid,
                        90.0,
                    )
                    .clicked()
                        || (enter && valid)
                    {
                        submit = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(crate::i18n::text("interaction-forms-cancel"))
                                .min_size(vec2(80.0, 30.0)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if resp.should_close() && !asked {
            // Esc in the browser goes back to the form.
            if form.browser.is_some() {
                form.browser = None;
            } else {
                cancel = true;
            }
        }
        if cancel {
            self.remote_form = None;
        } else if submit {
            match form.target() {
                Ok((host, path)) => {
                    self.remote_form = None;
                    let label = format!("{path} ({})", host.label());
                    self.connect_with(label, None, ctx, move || {
                        let path = resolve_home(&host, &path)?;
                        repo_root(&remote::encode(&host, &path)).map_err(Into::into)
                    });
                }
                Err(e) => form.error = Some(e),
            }
        }
    }
}

/// `path` (on `host`) with a leading `~` made the home folder there: a `\\wsl.localhost` path
/// can't say `~`.
fn resolve_home(host: &Host, path: &str) -> Result<String, String> {
    let Some(rest) = path.strip_prefix('~') else {
        return Ok(path.to_owned());
    };
    let out = remote::exec(
        host,
        &remote::Request {
            script: "printf '%s' \"$HOME\"".to_owned(),
            ..remote::Request::default()
        },
    )?;
    let home = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.code != 0 || !home.starts_with('/') {
        return Err(format!(
            "could not find the home folder on {}",
            host.label()
        ));
    }
    Ok(format!("{home}{rest}"))
}

#[cfg(test)]
#[path = "remote_open/localization_tests.rs"]
mod localization_tests;
#[cfg(test)]
#[path = "remote_open/tests.rs"]
mod tests;
