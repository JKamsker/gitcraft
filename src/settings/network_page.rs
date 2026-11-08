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
                "Inspect the effective helper and stored account for each HTTPS remote. Secrets are never displayed. A credential store may be shared with other profiles and applications."
            } else {
                "Test a repository's actual route, including proxies, custom CAs and the selected profile. SSH and WSL repositories are checked on their own machine. Tests do not prompt for sign-in."
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
                ui.add_enabled_ui(!busy,|ui|{egui::ComboBox::from_id_salt("network-repo").selected_text(if s.repo.as_os_str().is_empty(){"Choose repository".into()}else{s.repo.display().to_string()}).show_ui(ui,|ui|{for p in &paths {ui.selectable_value(&mut s.repo,p.clone(),p.display().to_string());}});});
                if s.repo!=before {s.inventory=Default::default();s.remote.clear();s.lines.clear();s.account=None;s.confirm_reject=false;}
                if s.repo.as_os_str().is_empty(){layout::note(ui,"Open a repository or choose one from this profile's recent repositories.");}
                if ui.add_enabled(!busy&&!s.repo.as_os_str().is_empty(),egui::Button::new("Inspect routes and remotes")).clicked(){let p=s.repo.clone();let name=s.remote.clone();s.launch(ui.ctx(),move||doctor::inventory(&p,Some(&name)).map(Answer::Inventory));}
                let remotes=s.inventory.remotes.clone();let before=s.remote.clone();
                ui.add_enabled_ui(!busy,|ui|{egui::ComboBox::from_id_salt("network-remote").selected_text(if s.remote.is_empty(){"Choose remote"}else{&s.remote}).show_ui(ui,|ui|{for e in &remotes{if (https && e.url.starts_with("https://")) || (!https && !e.push && !e.lfs){ui.selectable_value(&mut s.remote,e.key(),format!("{} · {}",e.key(),doctor::endpoint(&e.url)));}}});});
                if before!=s.remote{s.lines.clear();s.account=None;s.confirm_reject=false;}
                let selected=remotes.iter().find(|e|e.key()==s.remote && (https || (!e.push&&!e.lfs))).cloned();
                if https {
                    let selected=selected.filter(|e|valid_https(&e.url));
                    if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new("Inspect helper and account")).clicked() && let Some(endpoint)=selected.as_ref(){let p=s.repo.clone();let url=endpoint.url.clone();s.launch(ui.ctx(),move||{let helpers=crate::git::connection_read(&p,&["config","--get-urlmatch","credential.helper",&url])?;let mut lines=helper_lines(&helpers);let account=crate::git::https_account(&p,&url)?;
                        lines.push("Helpers actually invoked by Git for this lookup (in order):".into());
                        if account.helpers.is_empty(){lines.push("none invoked".into());}else{lines.extend(account.helpers);}
                        Ok(Answer::Account(lines,account.username))});}
                    if let Some(account)=&s.account{ui.label(format!("Stored account: {account}"));}else{layout::note(ui,"No stored account inspected, or the helper supplied none without prompting.");}
                    if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new("Sign out / switch account…")).clicked(){s.confirm_reject=true;}
                    if s.confirm_reject {layout::note(ui,"This runs git credential reject for this remote URL, following credential.useHttpPath. A shared helper may remove the host's account for other applications too.");ui.horizontal(|ui|{if ui.add_enabled(!busy,egui::Button::new("Forget this sign-in")).clicked(){if let Some(endpoint)=selected{let url=endpoint.url;let p=s.repo.clone();s.launch(ui.ctx(),move||{crate::git::https_reject(&p,&url)?;Ok(Answer::Rejected(p,url))});}s.confirm_reject=false;}
                        if ui.button("Keep it").clicked(){s.confirm_reject=false;}});}
                } else if ui.add_enabled(!busy&&selected.is_some(),egui::Button::new("Test connection")).clicked(){let p=s.repo.clone();let name=s.remote.clone();s.launch(ui.ctx(),move||doctor::test(&p,&name).map(Answer::Test));}
                if busy&&ui.button("Cancel check").clicked(){s.jobs.0.cancel();s.jobs=Default::default();s.job=None;s.lines=vec!["Check cancelled".into()];}
                for line in &s.lines{ui.label(line);}
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_commands_and_urls_never_leak() {
        let out = helper_lines("manager\n!echo token-secret\n/path/to/custom --secret");
        assert!(out.iter().any(|s| s == "manager"));
        assert!(!format!("{out:?}").contains("token-secret"));
        assert!(!valid_https("https://host/\npassword=x"));
    }
}
