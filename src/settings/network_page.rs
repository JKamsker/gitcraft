//! Network and HTTPS account controls. No command runs while painting or counting search rows.
use super::{
    Settings,
    layout::{self, Filter, section},
};
use crate::{connection_doctor as doctor, tasks};
use egui::{Id, Ui};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver},
    },
};

#[derive(Default)]
struct State {
    context: crate::connections::Context,
    repo: PathBuf,
    remote: String,
    inventory: doctor::Inventory,
    lines: Vec<String>,
    account: Option<String>,
    confirm_reject: bool,
    job: Option<Receiver<Result<Answer, String>>>,
    jobs: tasks::Scope,
}
enum Answer {
    Inventory(doctor::Inventory),
    Test(doctor::Report),
    Account(Vec<String>, Option<String>),
    Rejected(PathBuf, String),
}
fn state_id() -> Id {
    Id::new("network-settings-state")
}
pub(crate) fn repositories(ctx: &egui::Context, paths: Vec<PathBuf>) {
    ctx.data_mut(|d| d.insert_temp(Id::new("network-settings-repositories"), paths));
}
pub(crate) fn take_rejections(ctx: &egui::Context) -> Vec<(PathBuf, String)> {
    ctx.data_mut(|d| d.remove_temp::<Vec<(PathBuf, String)>>(Id::new("https-rejections")))
        .unwrap_or_default()
}
pub(crate) fn rejected(ctx: &egui::Context, repo: PathBuf, url: String) {
    ctx.data_mut(|d| {
        let key = Id::new("https-rejections");
        let mut events = d
            .get_temp::<Vec<(PathBuf, String)>>(key)
            .unwrap_or_default();
        events.push((repo, url));
        d.insert_temp(key, events);
    });
}

/// A secondary consumer has no operation-bound transient retry policy. Its actual
/// endpoint remains inspectable through an explicit standalone Settings action.
pub(crate) fn inspect_secondary(ctx: &egui::Context, repo: PathBuf, urls: Vec<String>) {
    let context = crate::connections::current();
    let remotes = urls
        .into_iter()
        .enumerate()
        .filter(|(_, url)| valid_https(url))
        .map(|(index, url)| crate::git::RemoteEndpoint {
            remote: "Reported secondary endpoint".into(),
            push: false,
            lfs: true,
            index,
            url,
        })
        .collect::<Vec<_>>();
    let selected = remotes.first().map(|e| e.key()).unwrap_or_default();
    let inventory=doctor::Inventory{remotes,lines:vec!["This LFS/secondary sign-in is separate from the failed Git operation. Inspect or forget only the endpoint you explicitly select here, select its account in Profile connections or your credential manager, then run the operation again. No automatic retry was authorized.".into()]};
    let lines = inventory.lines.clone();
    let state = Arc::new(Mutex::new(State {
        context,
        repo,
        remote: selected,
        inventory,
        lines,
        ..Default::default()
    }));
    ctx.data_mut(|data| {
        data.insert_temp(state_id(), state);
        data.insert_temp(Id::new("https-inspection-request"), true);
    });
}
pub(crate) fn take_inspection_request(ctx: &egui::Context) -> bool {
    ctx.data_mut(|data| data.remove_temp::<bool>(Id::new("https-inspection-request")))
        .unwrap_or(false)
}

impl State {
    fn launch(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<Answer, String> + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let wake = ctx.clone();
        let result = tasks::pool().and_then(|pool| {
            pool.submit(
                self.jobs.0.child(),
                tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let _ = tx.send(work());
                    wake.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match result {
            Ok(()) => {
                self.lines = vec!["Working…".into()];
                self.job = Some(rx);
            }
            Err(e) => self.lines = vec![e],
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.job.as_ref() else { return };
        let answer = match rx.try_recv() {
            Ok(a) => a,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Connection check stopped".into()),
        };
        self.job = None;
        match answer {
            Ok(Answer::Inventory(i)) => {
                if !i.remotes.iter().any(|e| e.key() == self.remote) {
                    self.remote = i.remotes.first().map(|e| e.key()).unwrap_or_default();
                }
                self.lines = i.lines.clone();
                self.inventory = i;
            }
            Ok(Answer::Test(r)) => {
                self.lines = r.lines;
                if !r.success {
                    self.lines.push("No configuration has been changed.".into());
                }
            }
            Ok(Answer::Account(lines, account)) => {
                self.lines = lines;
                self.account = account;
            }
            Ok(Answer::Rejected(repo, url)) => {
                self.account = None;
                self.lines=vec!["Asked this repository's effective credential helpers to forget the selected HTTPS sign-in. The next network operation can sign in again. Named forge accounts selected by the profile must be changed in Profile connections.".into()];
                rejected(ctx, repo, url);
            }
            Err(e) => self.lines = vec![crate::diagnostics::crash::scrub(&e)],
        }
    }
}
impl Settings {
    pub(super) fn network_ui(&mut self, ui: &mut Ui, f: &Filter, https: bool) {
        section(
            ui,
            f,
            if https {
                "HTTPS sign-ins"
            } else {
                "Connection doctor"
            },
            Some(if https {
                crate::i18n::text("interaction-accounts-https-intro")
            } else {
                crate::i18n::text("interaction-accounts-doctor-intro")
            }),
            |card| {
                card.row(if https{"HTTPS accounts"}else{"Repository network"},"proxy DNS TCP TLS authentication credential helper sign out switch account ls-remote",|ui|{
                let context=crate::connections::current();
                let state:Arc<Mutex<State>>=ui.data_mut(|d|d.get_temp(state_id())).unwrap_or_else(||Arc::new(Mutex::new(State::default())));
                let mut s=state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if s.context!=context {*s=State{context,..Default::default()};}
                s.poll(ui.ctx());
                let paths:Vec<PathBuf>=ui.data(|d|d.get_temp(Id::new("network-settings-repositories"))).unwrap_or_default();
                let busy=s.job.is_some();
                let before=s.repo.clone();
                ui.add_enabled_ui(!busy,|ui|{egui::ComboBox::from_id_salt("network-repo").selected_text(if s.repo.as_os_str().is_empty(){crate::i18n::text("interaction-accounts-choose-repo").into()}else{s.repo.display().to_string()}).show_ui(ui,|ui|{for p in &paths {ui.selectable_value(&mut s.repo,p.clone(),p.display().to_string());}});});
                if s.repo!=before {s.inventory=Default::default();s.remote.clear();s.lines.clear();s.account=None;s.confirm_reject=false;}
                if s.repo.as_os_str().is_empty(){layout::note(ui,crate::i18n::text("interaction-accounts-open-repo"));}
                if ui.add_enabled(!busy&&!s.repo.as_os_str().is_empty(),egui::Button::new(crate::i18n::text("interaction-accounts-inspect-routes"))).clicked(){let p=s.repo.clone();let name=s.remote.clone();s.launch(ui.ctx(),move||doctor::inventory(&p,Some(&name)).map(Answer::Inventory));}
                let remotes=s.inventory.remotes.clone();let before=s.remote.clone();
                ui.add_enabled_ui(!busy,|ui|{egui::ComboBox::from_id_salt("network-remote").selected_text(if s.remote.is_empty(){crate::i18n::text("interaction-accounts-choose-remote")}else{&s.remote}).show_ui(ui,|ui|{for e in &remotes{if (https && e.url.starts_with("https://")) || (!https && !e.push && !e.lfs){ui.selectable_value(&mut s.remote,e.key(),format!("{} · {}",e.key(),doctor::endpoint(&e.url)));}}});});
                if before!=s.remote{s.lines.clear();s.account=None;s.confirm_reject=false;}
                let selected=remotes.iter().find(|e|e.key()==s.remote && (https || (!e.push&&!e.lfs))).cloned();
                if https {
                    let selected=selected.filter(|e|valid_https(&e.url));
                    if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new(crate::i18n::text("interaction-accounts-inspect-helper"))).clicked() && let Some(endpoint)=selected.as_ref(){let p=s.repo.clone();let url=endpoint.url.clone();s.launch(ui.ctx(),move||{let helpers=crate::git::connection_read(&p,&["config","--get-urlmatch","credential.helper",&url])?;let mut lines=helper_lines(&helpers);let account=crate::git::https_account(&p,&url)?;
                        lines.push("Helpers actually invoked by Git for this lookup (in order):".into());
                        if account.helpers.is_empty(){lines.push("none invoked".into());}else{lines.extend(account.helpers);}
                        Ok(Answer::Account(lines,account.username))});}
                    if let Some(account)=&s.account{ui.label(crate::i18n::message("interaction-accounts-stored-account", &[("account", account.as_str().into())]));}else{layout::note(ui,crate::i18n::text("interaction-accounts-no-account"));}
                    if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new(crate::i18n::text("interaction-accounts-sign-out"))).clicked(){s.confirm_reject=true;}
                    if s.confirm_reject {layout::note(ui,crate::i18n::text("interaction-accounts-reject-help"));ui.horizontal(|ui|{if ui.add_enabled(!busy,egui::Button::new(crate::i18n::text("interaction-accounts-forget-login"))).clicked(){if let Some(endpoint)=selected{let url=endpoint.url;let p=s.repo.clone();s.launch(ui.ctx(),move||{crate::git::https_reject(&p,&url)?;Ok(Answer::Rejected(p,url))});}s.confirm_reject=false;}
                        if ui.button(crate::i18n::text("interaction-accounts-keep-login")).clicked(){s.confirm_reject=false;}});}
                } else if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new(crate::i18n::text("interaction-accounts-test"))).clicked(){let p=s.repo.clone();let name=s.remote.clone();s.launch(ui.ctx(),move||doctor::test(&p,&name).map(Answer::Test));}
                if busy&&ui.button(crate::i18n::text("interaction-accounts-cancel")).clicked(){s.jobs.0.cancel();s.jobs=Default::default();s.job=None;s.lines=vec!["Check cancelled".into()];}
                for line in &s.lines{ui.label(display_line(line));}
                drop(s);ui.data_mut(|d|d.insert_temp(state_id(),state));
            });
            },
        );
    }
}
pub(crate) fn valid_https(url: &str) -> bool {
    url.starts_with("https://") && !url.chars().any(char::is_control) && url.len() < 32768
}
fn helper_lines(value: &str) -> Vec<String> {
    let mut lines = vec!["Last URL-matched configured helper:".into()];
    for helper in value.lines() {
        let h = helper.trim();
        lines.push(if h.is_empty() {
            "reset helper chain".into()
        } else if [
            "manager",
            "manager-core",
            "osxkeychain",
            "cache",
            "store",
            "libsecret",
            "wincred",
        ]
        .contains(&h)
        {
            h.into()
        } else {
            "custom or profile helper (command and arguments withheld)".into()
        });
    }
    if value.is_empty() {
        lines.push("none configured".into());
    }
    lines
}
/// Render only known app-owned diagnostic prose; external/raw data remains opaque.
fn display_line(raw: &str) -> String {
    let translated = match raw {
        "This LFS/secondary sign-in is separate from the failed Git operation. Inspect or forget only the endpoint you explicitly select here, select its account in Profile connections or your credential manager, then run the operation again. No automatic retry was authorized." => {
            crate::i18n::text("interaction-accounts-secondary")
        }
        "Working…" => crate::i18n::text("interaction-accounts-working"),
        "Connection check stopped" => crate::i18n::text("interaction-accounts-stopped"),
        "No configuration has been changed." => crate::i18n::text("interaction-accounts-unchanged"),
        "Asked this repository's effective credential helpers to forget the selected HTTPS sign-in. The next network operation can sign in again. Named forge accounts selected by the profile must be changed in Profile connections." => {
            crate::i18n::text("interaction-accounts-rejected")
        }
        "Helpers actually invoked by Git for this lookup (in order):" => {
            crate::i18n::text("interaction-accounts-invoked")
        }
        "none invoked" => crate::i18n::text("interaction-accounts-none-invoked"),
        "Check cancelled" => crate::i18n::text("interaction-accounts-cancelled"),
        "Last URL-matched configured helper:" => {
            crate::i18n::text("interaction-accounts-last-helper")
        }
        "reset helper chain" => crate::i18n::text("interaction-accounts-reset"),
        "custom or profile helper (command and arguments withheld)" => {
            crate::i18n::text("interaction-accounts-custom-helper")
        }
        "none configured" => crate::i18n::text("interaction-accounts-none-configured"),
        _ => return doctor::display_line(raw),
    };
    translated.to_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_idle_network_page_translates_status_at_paint_and_keeps_raw_state()
    -> Result<(), String> {
        use egui_kittest::{Harness, kittest::Queryable};
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let state = Arc::new(Mutex::new(State {
            context: crate::connections::current(),
            lines: vec![
                "Check cancelled".into(),
                "external diagnostic 日本語".into(),
                "TLS chain: NOT verified — certificate verification is disabled in Git's configuration or environment".into(),
                "ls-remote: failed in 2.34 s".into(),
            ],
            ..State::default()
        }));
        let recorded = state.clone();
        let mut h = Harness::new_ui_state(
            move |ui, settings: &mut Settings| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                ui.data_mut(|d| d.insert_temp(state_id(), state.clone()));
                settings.network_ui(ui, &Filter::new("", "HTTPS sign-ins", false), true);
            },
            Settings::default(),
        );
        h.run();
        h.get_by_label("HTTPS-Konten");
        h.get_by_value("Repository auswählen");
        h.get_by_label("Prüfung abgebrochen");
        h.get_by_label("external diagnostic 日本語");
        h.get_by_label("TLS-Zertifikatskette: NICHT geprüft — die Zertifikatsprüfung ist in der Git-Konfiguration oder -Umgebung deaktiviert");
        h.get_by_label("ls-remote: nach 2,34 s fehlgeschlagen");
        assert_eq!(
            recorded.lock().map_err(|error| format!("{error:?}"))?.lines[0],
            "Check cancelled"
        );
        assert!(
            recorded
                .lock()
                .map_err(|error| format!("{error:?}"))?
                .job
                .is_none()
        );
        let raw_helpers = helper_lines("manager\n!echo secret");
        assert_eq!(raw_helpers[0], "Last URL-matched configured helper:");
        assert_eq!(
            display_line(&raw_helpers[0]),
            "Letzter konfigurierter Helper mit passender URL:"
        );
        assert_eq!(raw_helpers[1], "manager");
        assert_eq!(display_line("manager"), "manager");
        assert!(!raw_helpers.join(" ").contains("secret"));
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Check cancelled");
        assert_eq!(
            recorded.lock().map_err(|error| format!("{error:?}"))?.lines[0],
            "Check cancelled"
        );
        h.get_by_label("TLS chain: NOT verified — certificate verification is disabled in Git's configuration or environment");
        h.get_by_label("ls-remote: failed in 2.34 s");
        assert_eq!(
            recorded.lock().map_err(|error| format!("{error:?}"))?.lines[3],
            "ls-remote: failed in 2.34 s"
        );

        Ok(())
    }

    #[test]
    fn helper_commands_and_urls_never_leak() {
        let out = helper_lines("manager\n!echo token-secret\n/path/to/custom --secret");
        assert!(out.iter().any(|s| s == "manager"));
        assert!(!format!("{out:?}").contains("token-secret"));
        assert!(!valid_https("https://host/\npassword=x"));
    }
}
