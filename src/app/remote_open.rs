//! Opening a repository on another machine (WSL, SSH): the dialog, and the connection made in
//! the background (the first one to an SSH host takes a second or more).

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

/// The machine (WSL distribution or SSH host) and the folder there: whether Browse was clicked.
fn form_fields(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) -> (bool, bool) {
    let mut enter = false;
    ui.horizontal(|ui| {
        ui.selectable_value(&mut form.kind, Kind::Wsl, "WSL");
        ui.selectable_value(&mut form.kind, Kind::Ssh, "SSH");
    });
    ui.add_space(8.0);
    match form.kind {
        Kind::Wsl => distro_field(ui, form, t),
        Kind::Ssh => enter |= host_field(ui, form, t),
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-folder"))
            .color(t.text_dim)
            .small(),
    );
    let mut browse = false;
    ui.horizontal(|ui| {
        let path = ui.add(
            egui::TextEdit::singleline(&mut form.path)
                .hint_text(crate::i18n::text("interaction-forms-path-hint"))
                .desired_width(ui.available_width() - 90.0)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= super::clone::singleline_enter(&path);
        if std::mem::take(&mut form.focus) {
            path.request_focus();
        }
        if path.changed() {
            form.error = None;
        }
        browse = ui
            .add(
                egui::Button::new(crate::i18n::text("interaction-forms-browse"))
                    .min_size(vec2(80.0, 28.0)),
            )
            .on_hover_text(crate::i18n::text("interaction-forms-browse-tip"))
            .clicked();
    });
    (browse, enter)
}

fn distro_field(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) {
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-distro"))
            .color(t.text_dim)
            .small(),
    );
    if form.distros_rx.is_some() {
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(
                RichText::new(crate::i18n::text("interaction-forms-looking-distros"))
                    .color(t.text_faint)
                    .small(),
            );
        });
        return;
    }
    if form.distros.is_empty() {
        ui.label(
            RichText::new(crate::i18n::text("interaction-forms-no-distros"))
                .color(t.text_faint)
                .small(),
        );
        return;
    }
    egui::ComboBox::from_id_salt("remote-distro")
        .selected_text(form.distro.clone())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for d in &form.distros {
                ui.selectable_value(&mut form.distro, d.clone(), d);
            }
        });
}

fn host_field(ui: &mut Ui, form: &mut RemoteForm, t: &Tokens) -> bool {
    let mut enter = false;
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-host"))
            .color(t.text_dim)
            .small(),
    );
    ui.horizontal(|ui| {
        let combo_w = if form.hosts.is_empty() { 0.0 } else { 40.0 };
        let host = ui.add(
            egui::TextEdit::singleline(&mut form.host)
                .hint_text(crate::i18n::text("interaction-forms-host-hint"))
                .desired_width(ui.available_width() - combo_w)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        enter |= super::clone::singleline_enter(&host);
        if !form.hosts.is_empty() {
            egui::ComboBox::from_id_salt("remote-ssh-hosts")
                .selected_text("")
                .width(24.0)
                .show_ui(ui, |ui| {
                    for h in &form.hosts {
                        if ui.selectable_label(form.host == *h, h).clicked() {
                            form.host = h.clone();
                        }
                    }
                })
                .response
                .on_hover_text(crate::i18n::text("interaction-forms-hosts-tip"));
        }
    });
    ui.label(
        RichText::new(crate::i18n::text("interaction-forms-keys"))
            .color(t.text_faint)
            .small(),
    );
    enter
}

/// Known connection presentation only; host/path validation and raw errors stay stable.
pub(super) fn display_message(raw: &str) -> Option<String> {
    use crate::i18n::{message, text};
    let id = match raw {
        "The folder is a path on that machine: /home/me/repo or ~/repo." => {
            Some("interaction-forms-remote-path")
        }
        "Pick a WSL distribution." => Some("interaction-forms-remote-distro"),
        "The host is what ssh takes: an alias of ~/.ssh/config or user@host." => {
            Some("interaction-forms-remote-host")
        }
        "Connecting failed unexpectedly." => Some("interaction-forms-remote-failed"),
        _ => None,
    };
    if let Some(id) = id {
        return Some(text(id).to_owned());
    }
    if let Some(place) = raw
        .strip_prefix("Connecting to ")
        .and_then(|s| s.strip_suffix('…'))
    {
        return Some(message(
            "interaction-forms-connecting",
            &[("place", place.into())],
        ));
    }
    raw.strip_prefix("could not find the home folder on ")
        .map(|machine| {
            message(
                "interaction-forms-remote-home",
                &[("machine", machine.into())],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(kind: Kind, path: &str) -> RemoteForm {
        RemoteForm {
            kind,
            distros: vec!["Ubuntu".to_owned()],
            distro: "Ubuntu".to_owned(),
            distros_rx: None,
            hosts: Vec::new(),
            host: "build".to_owned(),
            path: path.to_owned(),
            error: None,
            focus: false,
            browser: None,
        }
    }

    #[test]
    fn the_form_names_a_folder_there() {
        let build = || Host::Ssh("build".to_owned());
        assert_eq!(
            form(Kind::Ssh, " ~/priv/repo ").target(),
            Ok((build(), "~/priv/repo".to_owned()))
        );
        assert_eq!(
            form(Kind::Ssh, "/srv/x").target(),
            Ok((build(), "/srv/x".to_owned()))
        );
        assert!(form(Kind::Ssh, "relative").target().is_err());
        let mut bad = form(Kind::Ssh, "/x");
        bad.host = "-oProxyCommand=x".to_owned();
        assert!(bad.target().is_err());
    }

    #[test]
    fn a_wsl_folder_names_its_distribution() {
        assert_eq!(
            form(Kind::Wsl, "/home/me/repo").target(),
            Ok((Host::Wsl("Ubuntu".to_owned()), "/home/me/repo".to_owned()))
        );
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn enter_on_remote_cancel_closes_without_submitting() {
        crate::i18n::set_language("en-US");
        let mut h = Harness::new_ui_state(
            |ui, state: &mut Option<App>| {
                if state.is_none() {
                    let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                    let mut app = App::launched_using(&cc, None, || {
                        Ok("git version 2.43.0 (recorded)".into())
                    });
                    app.remote_form = Some(RemoteForm {
                        kind: Kind::Ssh,
                        distros: vec![],
                        distro: String::new(),
                        distros_rx: None,
                        hosts: vec![],
                        host: "-invalid".into(),
                        path: "/inert/repository".into(),
                        error: None,
                        focus: false,
                        browser: None,
                    });
                    *state = Some(app);
                    return;
                }
                if let Some(app) = state {
                    app.remote_dialog(ui.ctx());
                }
            },
            None,
        );
        h.run_steps(3);
        h.get_by_label("Cancel").focus();
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert!(
            h.state()
                .as_ref()
                .is_some_and(|app| app.remote_form.is_none() && app.connecting.is_empty())
        );
    }

    #[test]
    fn actual_remote_fields_switch_kind_with_opaque_targets_and_no_discovery() {
        crate::i18n::set_language("de");
        let form = RemoteForm {
            kind: Kind::Ssh,
            distros: vec!["Ubuntu-24.04".into()],
            distro: "Ubuntu-24.04".into(),
            distros_rx: None,
            hosts: vec![],
            host: "me@example.invalid".into(),
            path: "~/repo 日本語".into(),
            error: None,
            focus: false,
            browser: None,
        };
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, form: &mut RemoteForm| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                assert!(!form_fields(ui, form, &Tokens::get(ui.ctx())).0);
            },
            form,
        );
        h.run();
        h.get_by_label("Ordner dort");
        h.get_by_label("Es werden Schlüssel und ein Agent verwendet; ein Passwort oder eine Passphrase wird hier abgefragt.");
        h.get_by_label("WSL").click();
        h.run();
        assert_eq!(h.state().kind, Kind::Wsl);
        assert_eq!(
            h.state().target(),
            Ok((Host::Wsl("Ubuntu-24.04".into()), "~/repo 日本語".into()))
        );
        h.get_by_label("SSH").click();
        h.run();
        assert_eq!(
            h.state().target(),
            Ok((
                Host::Ssh("me@example.invalid".into()),
                "~/repo 日本語".into()
            ))
        );
        assert!(h.state().distros_rx.is_none() && h.state().browser.is_none());
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("Folder there");
    }

    #[test]
    fn remote_validation_and_status_are_display_only_and_keep_raw_machine_path_values() {
        crate::i18n::set_language("de");
        for (raw, expected) in [
            (
                "Pick a WSL distribution.",
                "Wähle eine WSL-Distribution aus.",
            ),
            (
                "The folder is a path on that machine: /home/me/repo or ~/repo.",
                "Der Ordner muss als Pfad auf dem anderen Rechner angegeben werden: /home/me/repo oder ~/repo.",
            ),
            (
                "The host is what ssh takes: an alias of ~/.ssh/config or user@host.",
                "Der Host wird wie bei ssh angegeben: ein Alias aus ~/.ssh/config oder user@host.",
            ),
            (
                "Connecting failed unexpectedly.",
                "Die Verbindung ist unerwartet fehlgeschlagen.",
            ),
            (
                "Connecting to ssh://me@example.invalid/日本語…",
                "Verbindung mit ssh://me@example.invalid/日本語 wird hergestellt…",
            ),
            (
                "could not find the home folder on SSH · me@example.invalid",
                "Der persönliche Ordner auf SSH · me@example.invalid konnte nicht ermittelt werden",
            ),
        ] {
            assert_eq!(display_message(raw).as_deref(), Some(expected));
        }
        assert!(display_message("External SSH diagnostic 日本語").is_none());
        crate::i18n::set_language("en-US");
    }
}
