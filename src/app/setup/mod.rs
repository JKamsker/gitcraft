//! Portable setup preview and background handoffs. Imported paths never choose destinations.
mod files;
#[cfg(test)]
mod localization_tests;
mod pending;
#[cfg(test)]
mod tests;

use super::{App, Tab, profiles::ProfileState};
use crate::{
    portability::{self, Outcome, Receipt, Setup},
    settings::{SavedIdentity, UserProfile},
    theme::ThemeId,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError, channel},
    },
};

#[derive(Clone)]
struct Candidate {
    path: PathBuf,
    profiles: Vec<u64>,
    hidden: Vec<String>,
    selected: bool,
}
#[derive(Default)]
struct Plan {
    selected: bool,
    name: String,
    parent: PathBuf,
    source: Option<PathBuf>,
    receipt: Option<Receipt>,
    messages: Vec<String>,
    complete: bool,
    remotes: Vec<portability::Remote>,
    ssh_addresses_confirmed: bool,
}
impl Plan {
    fn needs_ssh(&self) -> bool {
        self.remotes.iter().any(|r| {
            std::iter::once(&r.fetch)
                .chain(&r.push)
                .any(|url| !url.starts_with("https://"))
        })
    }
    fn binding_ready(&self, profile: &UserProfile) -> bool {
        !self.needs_ssh()
            || (!profile.connections.ssh_key.is_empty() && self.ssh_addresses_confirmed)
    }
}
struct Job {
    rx: Receiver<Result<Answer, String>>,
    active: Arc<AtomicBool>,
    cancel: crate::tasks::Cancellation,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.cancel.cancel();
    }
}
enum Answer {
    Snapshot(Setup, Vec<String>),
    Loaded(Setup),
    Saved(PathBuf),
    Restored(usize, Outcome),
    Verified(Box<(UserProfile, UserProfile)>),
}
#[derive(Default)]
pub(super) struct SetupWindow {
    pub open: bool,
    import: bool,
    candidates: Vec<Candidate>,
    file: Option<Arc<Setup>>,
    plans: Vec<Plan>,
    mappings: BTreeMap<u64, u64>,
    applied: bool,
    preferences: bool,
    job: Option<Job>,
    messages: Vec<String>,
    fresh: BTreeMap<u64, pending::Fresh>,
    jobs: crate::tasks::Scope,
}
impl SetupWindow {
    pub(super) fn forget_profile(&mut self, id: u64) {
        self.mappings.retain(|_, mapped| *mapped != id);
        self.fresh.remove(&id);
    }
    pub(super) fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn launch(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Fn() -> bool) -> Result<Answer, String> + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let active = Arc::new(AtomicBool::new(true));
        let worker = active.clone();
        let ctx = ctx.clone();
        let cancel = self.jobs.0.child();
        let scope = cancel.clone();
        let submit = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                cancel.clone(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let result = work(&|| worker.load(Ordering::Acquire) && !scope.is_cancelled());
                    let _ = tx.send(result);
                    ctx.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submit {
            Ok(()) => self.job = Some(Job { rx, active, cancel }),
            Err(e) => self.messages.push(e),
        }
    }
    fn take_answer(&mut self) -> Option<Result<Answer, String>> {
        let job = self.job.as_ref()?;
        let answer = match job.rx.try_recv() {
            Ok(a) => a,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(
                "Setup worker stopped; retained destinations are available for inspection".into(),
            ),
        };
        let active = job.active.load(Ordering::Acquire);
        self.job = None;
        if !active && !matches!(&answer, Ok(Answer::Restored(..) | Answer::Saved(..))) {
            return Some(Err(
                "Setup operation cancelled; stale results were discarded".into(),
            ));
        }
        Some(answer)
    }
    fn set_file(&mut self, mut file: Setup) {
        file.require_repository_key_setup();
        self.plans = file
            .repositories
            .iter()
            .map(|r| Plan {
                name: r.name.clone(),
                remotes: r.remotes.clone(),
                ..Default::default()
            })
            .collect();
        self.file = Some(Arc::new(file));
        self.mappings.clear();
        self.applied = false;
    }
}
fn preview_summary(ui: &mut egui::Ui, file: &Setup) {
    ui.label(crate::i18n::message(
        "interaction-setup-preview",
        &[
            ("profiles", file.profiles.len().into()),
            ("authors", file.identities.len().into()),
            ("repositories", file.repositories.len().into()),
        ],
    ));
    for p in &file.profiles {
        let id = if p.setup.pending() {
            "interaction-setup-profile-pending"
        } else {
            "interaction-setup-profile"
        };
        ui.label(crate::i18n::message(
            id,
            &[
                ("name", p.name.as_str().into()),
                ("author", p.author_name.as_str().into()),
                ("email", p.author_email.as_str().into()),
            ],
        ));
        for a in &p.setup.accounts {
            ui.label(crate::i18n::message(
                "interaction-setup-account",
                &[
                    ("provider", format!("{:?}", a.provider).into()),
                    ("user", a.user_or_alias.as_str().into()),
                    ("host", a.host.as_str().into()),
                ],
            ));
        }
    }
}
impl App {
    pub(super) fn show_setup(&mut self, import: bool, ctx: &egui::Context) {
        self.setup.open = true;
        self.setup.import = import;
        if !import && !self.setup.busy() {
            let mut paths: BTreeMap<PathBuf, Vec<u64>> = BTreeMap::new();
            for path in self.recent.iter().chain(self.tabs.iter().filter_map(|t| {
                if let Tab::Repo(r) = t {
                    Some(&r.path)
                } else {
                    None
                }
            })) {
                paths
                    .entry(path.clone())
                    .or_default()
                    .push(self.settings.active_profile);
            }
            for (id, state) in &self.profile_states {
                for path in state.recent.iter().chain(&state.open) {
                    paths.entry(path.clone()).or_default().push(*id);
                }
            }
            self.setup.candidates = paths
                .into_iter()
                .map(|(path, mut profiles)| {
                    profiles.sort_unstable();
                    profiles.dedup();
                    let hidden = crate::repo::RepoTab::portable_visibility(ctx, &path);
                    Candidate {
                        path,
                        profiles,
                        hidden,
                        selected: false,
                    }
                })
                .collect();
        }
    }
    pub(super) fn poll_setup(&mut self, ctx: &egui::Context) {
        let Some(result) = self.setup.take_answer() else {
            return;
        };
        match result {
            Err(e) => self.setup.messages.push(e),
            Ok(Answer::Snapshot(file, errors)) => {
                self.setup.messages = errors;
                self.setup.set_file(file);
                self.setup.messages.push("Preview ready. Only successfully read selected repositories are included; review the count and omissions before saving.".into());
            }
            Ok(Answer::Loaded(file)) => {
                self.setup.set_file(file);
                self.setup.messages=vec!["Preview loaded. No settings, repositories or authentication have been changed.".into()];
            }
            Ok(Answer::Saved(path)) => self
                .setup
                .messages
                .push(format!("Setup saved to {}", path.display())),
            Ok(Answer::Restored(index, outcome)) => {
                let complete = outcome.complete;
                let restored_root = outcome.receipt.as_ref().map(|r| r.root.clone());
                if let Some(plan) = self.setup.plans.get_mut(index) {
                    plan.receipt = outcome.receipt;
                    plan.messages = outcome.messages;
                    plan.complete = outcome.complete;
                    if let (Some(receipt), Some(file)) = (&plan.receipt, &self.setup.file)
                        && let Some(r) = file.repositories.get(index)
                    {
                        crate::repo::RepoTab::restore_portable_visibility(
                            ctx,
                            &receipt.root,
                            &r.hidden,
                        );
                    }
                }
                if complete
                    && let (Some(root), Some(file)) = (restored_root, &self.setup.file)
                    && let Some(repo) = file.repositories.get(index)
                {
                    let mut owners = repo
                        .profile_ids
                        .iter()
                        .filter_map(|id| self.setup.mappings.get(id))
                        .copied()
                        .collect::<Vec<_>>();
                    if owners.is_empty() {
                        owners.push(self.settings.active_profile);
                    }
                    for id in owners {
                        let recent = if id == self.settings.active_profile {
                            &mut self.recent
                        } else {
                            &mut self.profile_states.entry(id).or_default().recent
                        };
                        recent.retain(|p| p != &root);
                        recent.insert(0, root.clone());
                        recent.truncate(super::MAX_RECENT);
                    }
                }
            }
            Ok(Answer::Verified(result)) => {
                let (before, ready) = *result;
                if let Some(profile) = self
                    .settings
                    .user_profiles
                    .iter_mut()
                    .find(|p| **p == before)
                {
                    *profile = ready;
                    self.settings.set(ctx);
                    self.setup.messages.push("Profile setup verified. Activate it explicitly before cloning its repositories.".into());
                } else {
                    self.setup.messages.push(
                        "Profile changed during verification; check the current profile again."
                            .into(),
                    );
                }
            }
        }
    }
    fn apply_setup_public(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.setup.applied {
            return Ok(());
        }
        let file = self
            .setup
            .file
            .as_ref()
            .ok_or("Load an import preview first")?;
        file.validate()?;
        let mut next = self
            .settings
            .user_profiles
            .iter()
            .map(|p| p.id)
            .max()
            .unwrap_or(0);
        let mut added = Vec::new();
        let mut states = Vec::new();
        let mut mappings = BTreeMap::new();
        for public in &file.profiles {
            next = next.checked_add(1).ok_or("Profile ID space is exhausted")?;
            let p = public.materialize(next);
            mappings.insert(public.source_id, next);
            states.push((
                next,
                ProfileState {
                    theme: ThemeId(public.theme.clone()),
                    ..Default::default()
                },
            ));
            added.push(p);
        }
        if self.setup.preferences {
            file.preferences.apply(&mut self.settings);
        }
        self.settings.user_profiles.extend(added);
        self.apply_theme(ctx);
        self.profile_states.extend(states);
        for identity in &file.identities {
            let identity = SavedIdentity {
                label: identity.label.clone(),
                name: identity.name.clone(),
                email: identity.email.clone(),
            };
            if !self.settings.identities.contains(&identity) {
                self.settings.identities.push(identity);
            }
        }
        self.setup.mappings = mappings;
        self.setup.applied = true;
        self.settings.set(ctx);
        self.setup.messages.push("Public settings and new profiles added. Your active profile is unchanged. Authentication, keys and signing programs require local setup.".into());
        Ok(())
    }
    pub(super) fn setup_ui(&mut self, ctx: &egui::Context) {
        if !self.setup.open {
            return;
        }
        let busy = self.setup.busy();
        let mut close = false;
        let mut export = false;
        let mut load = false;
        let mut save = false;
        let mut apply = false;
        let mut restore = None;
        let mut activate = None;
        let mut open = None;
        let mut verify = None;
        let modal=egui::Modal::new(egui::Id::new("portable-setup")).show(ctx,|ui|{
            ui.set_width(760.0_f32.min((ctx.content_rect().width()-48.0).max(240.0)));
            ui.heading(crate::i18n::text("interaction-setup-title"));
            ui.label(crate::i18n::text("interaction-setup-retry-info"));
            ui.label(crate::i18n::text("interaction-setup-intro"));
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()*0.72).max(150.0)).show(ui,|ui|{
                ui.add_enabled_ui(!busy,|ui|{ui.horizontal(|ui|{ui.selectable_value(&mut self.setup.import,false,crate::i18n::text("interaction-setup-export"));ui.selectable_value(&mut self.setup.import,true,crate::i18n::text("interaction-setup-import"));});
                    if !self.setup.import {
                        ui.label(crate::i18n::text("interaction-setup-selection"));
                        for candidate in &mut self.setup.candidates {ui.checkbox(&mut candidate.selected,candidate.path.display().to_string());}
                        export=ui.button(crate::i18n::text("interaction-setup-build")).clicked();
                    }else{load=ui.button(crate::i18n::text("interaction-setup-load")).clicked();}
                });
                if let Some(file)=self.setup.file.clone(){
                    ui.separator();preview_summary(ui,&file);
                    ui.add_enabled_ui(!busy,|ui|{if self.setup.import{ui.checkbox(&mut self.setup.preferences,crate::i18n::text("interaction-setup-preferences"));let response=ui.add_enabled(!self.setup.applied,egui::Button::new(crate::i18n::text("interaction-setup-add")));
                        apply=response.clicked();}else{save=ui.button(crate::i18n::text("interaction-setup-save")).clicked();}});
                    for (index,r) in file.repositories.iter().enumerate(){
                        ui.push_id(index,|ui|{ui.group(|ui|{
                            ui.label(crate::i18n::message("interaction-setup-previous", &[("name", r.name.as_str().into()), ("path", r.previous_path.as_str().into())]));
                            ui.label(crate::i18n::message("interaction-setup-checkout", &[("branch", r.branch.as_deref().unwrap_or(crate::i18n::text("interaction-setup-detached")).into()), ("worktrees", r.worktrees.len().into()), ("hidden", r.hidden.len().into()), ("config", r.config.len().into())]));
                            if !self.setup.import {for remote in &r.remotes {ui.label(format!("{}: {}",remote.name,remote.fetch));for url in &remote.push{ui.label(crate::i18n::message("interaction-setup-push", &[("url", url.as_str().into())]));}}}
                            for w in &r.worktrees{ui.label(crate::i18n::message("interaction-setup-worktree", &[("name", w.name.as_str().into()), ("branch", w.branch.as_deref().unwrap_or(crate::i18n::text("interaction-setup-detached")).into()), ("commit", w.commit.as_str().into()), ("path", w.previous_path.as_str().into())]));}
                            for notice in &r.notices {ui.label(notice);}
                            if self.setup.import {
                                let matching=r.profile_ids.iter().filter_map(|id|self.setup.mappings.get(id)).copied().collect::<Vec<_>>();
                                let permitted=self.setup.applied && (r.profile_ids.is_empty() || matching.contains(&self.settings.active_profile)) && !self.settings.profile().pending_setup();
                                let current_profile=self.settings.profile();
                                let plan=&mut self.setup.plans[index];
                                ui.add_enabled_ui(!busy,|ui|{
                                    ui.checkbox(&mut plan.selected,crate::i18n::text("interaction-setup-select"));
                                    ui.add_enabled_ui(plan.receipt.is_none(),|ui|{
                                        ui.label(crate::i18n::text("interaction-setup-addresses"));
                                        let mut changed=false;
                                        for remote in &mut plan.remotes {
                                            ui.label(crate::i18n::message("interaction-setup-remote", &[("name", remote.name.as_str().into())]));
                                            ui.horizontal(|ui|{ui.label(crate::i18n::text("interaction-setup-fetch"));changed|=ui.add(egui::TextEdit::singleline(&mut remote.fetch).char_limit(4096)).changed();});
                                            for (n,url) in remote.push.iter_mut().enumerate(){ui.horizontal(|ui|{ui.label(crate::i18n::message("interaction-setup-push-address", &[("number", (n+1).into())]));changed|=ui.add(egui::TextEdit::singleline(url).char_limit(4096)).changed();});}
                                        }
                                        if changed {plan.ssh_addresses_confirmed=false;}
                                        if plan.needs_ssh(){
                                            ui.checkbox(&mut plan.ssh_addresses_confirmed,crate::i18n::text("interaction-setup-confirm-ssh"));
                                            if current_profile.connections.ssh_key.is_empty(){ui.label(crate::i18n::text("interaction-setup-ssh-pending"));}
                                        }
                                    });
                                    if plan.source.is_none() && let Some(remote)=plan.remotes.first(){ui.label(crate::i18n::message("interaction-setup-clone-source", &[("source", remote.fetch.as_str().into())]));}
                                    ui.add_enabled_ui(plan.receipt.is_none(),|ui|{
                                        ui.horizontal(|ui|{ui.label(crate::i18n::text("interaction-setup-folder"));ui.text_edit_singleline(&mut plan.name);});
                                        if ui.button(crate::i18n::text("interaction-setup-parent")).clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){plan.parent=path;}
                                        if ui.button(crate::i18n::text("interaction-setup-source")).clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){plan.source=Some(path);}
                                        if plan.source.is_some() && ui.button(crate::i18n::text("interaction-setup-use-remote")).clicked(){plan.source=None;}
                                    });
                                    ui.label(crate::i18n::message("interaction-setup-destination", &[("path", plan.parent.join(&plan.name).display().to_string().into())]));
                                    if let Some(source)=&plan.source{ui.label(crate::i18n::message("interaction-setup-current-source", &[("path", source.display().to_string().into())]));}
                                    if !r.worktrees.is_empty(){ui.label(crate::i18n::message("interaction-setup-worktree-root", &[("path", plan.parent.join(format!("{}-worktrees",plan.name)).display().to_string().into())]));}
                                    if !permitted {ui.label(crate::i18n::text("interaction-setup-activate-help"));for id in &matching {if let Some(p)=self.settings.user_profiles.iter().find(|p|p.id==*id && !p.pending_setup()) && ui.button(crate::i18n::message("interaction-setup-activate", &[("profile", p.display_title().into())])).clicked(){activate=Some(*id);}}}
                                    if ui.add_enabled(plan.selected && permitted && plan.binding_ready(&current_profile) && !plan.parent.as_os_str().is_empty() && !plan.complete,egui::Button::new(if plan.receipt.is_some(){crate::i18n::text("interaction-setup-retry")}else{crate::i18n::text("interaction-setup-restore")})).clicked(){restore=Some(index);}
                                    if plan.receipt.is_some() && ui.button(crate::i18n::text("interaction-setup-new-destination")).clicked(){plan.receipt=None;plan.complete=false;plan.parent=PathBuf::new();plan.name=format!("{}-retry",r.name);plan.messages.push("Previous clone/worktrees were retained. Choose a new unused destination.".into());}
                                    if plan.complete && ui.button(crate::i18n::text("interaction-setup-open")).clicked(){open=plan.receipt.as_ref().map(|r|r.root.clone());}
                                });
                                for message in &plan.messages{ui.label(message);}
                            }
                        });});
                    }
                }
                ui.separator();ui.heading(crate::i18n::text("interaction-setup-finish"));
                ui.label(crate::i18n::text("interaction-setup-pending-help"));
                for profile in self.settings.user_profiles.iter().filter(|p|p.pending_setup()) {ui.push_id(profile.id,|ui|{ui.group(|ui|{ui.label(profile.display_title());let fresh=self.setup.fresh.entry(profile.id).or_default();ui.add_enabled_ui(!busy,|ui|{if pending::fields(ui,profile,fresh){verify=Some((profile.clone(),fresh.clone()));}});});});}
                for message in &self.setup.messages{ui.label(message);}
            });
            if busy {ui.horizontal(|ui|{ui.add(crate::spinner::Spinner::new());ui.label(crate::i18n::text("interaction-setup-working"));if ui.button(crate::i18n::text("interaction-setup-cancel")).clicked() && let Some(job)=&self.setup.job{job.active.store(false,Ordering::Release);job.cancel.cancel();}});}
            close=ui.add_enabled(!busy,egui::Button::new(crate::i18n::text("interaction-setup-close"))).clicked();
        });
        if close || (!busy && modal.should_close()) {
            self.setup.open = false;
        }
        if load
            && let Some(path) = rfd::FileDialog::new()
                .add_filter(crate::i18n::text("interaction-setup-filter"), &["json"])
                .pick_file()
        {
            self.setup.launch(ctx, move |active| {
                files::load(&path, active).map(Answer::Loaded)
            });
        }
        if export {
            let mut themes = self
                .profile_states
                .iter()
                .map(|(id, state)| (*id, state.theme.clone()))
                .collect::<BTreeMap<_, _>>();
            themes.insert(self.settings.active_profile, self.theme.clone());
            let mut file = Setup::new(&self.settings, &themes);
            let candidates = self
                .setup
                .candidates
                .iter()
                .filter(|c| c.selected)
                .cloned()
                .collect::<Vec<_>>();
            self.setup.launch(ctx, move |active| {
                file.resolve_public_accounts()?;
                let mut errors = Vec::new();
                for c in candidates {
                    if !active() {
                        return Err("Export cancelled".into());
                    }
                    match portability::snapshot(&c.path, c.profiles, c.hidden, active) {
                        Ok(mut repo) => {
                            let base = repo.name.clone();
                            let mut n = 2;
                            while file.repositories.iter().any(|r| r.name == repo.name) {
                                repo.name = format!("{base}-{n}");
                                n += 1;
                            }
                            file.repositories.push(repo);
                        }
                        Err(e) => errors.push(format!("Omitted {}: {e}", c.path.display())),
                    }
                }
                file.validate()?;
                Ok(Answer::Snapshot(file, errors))
            });
        }
        if save
            && let (Some(file), Some(path)) = (
                self.setup.file.clone(),
                rfd::FileDialog::new()
                    .set_file_name("gitcraft-setup.json")
                    .save_file(),
            )
        {
            self.setup.launch(ctx, move |active| {
                files::save(&path, &file, active)?;
                Ok(Answer::Saved(path))
            });
        }
        if apply && let Err(e) = self.apply_setup_public(ctx) {
            self.setup.messages.push(e);
        }
        if let Some(id) = activate {
            self.switch_profile(id, ctx);
        }
        if let Some(path) = open {
            self.open_root(path, ctx);
            self.setup.open = false;
        }
        if let Some(index) = restore
            && let Some(file) = &self.setup.file
        {
            let plan = &self.setup.plans[index];
            let mut repo = file.repositories[index].clone();
            repo.name = plan.name.clone();
            repo.remotes = plan.remotes.clone();
            let parent = plan.parent.clone();
            let source = plan.source.clone();
            let previous = plan.receipt.clone();
            self.setup.launch(ctx, move |active| {
                Ok(Answer::Restored(
                    index,
                    portability::restore(&repo, &parent, source.as_deref(), previous, active),
                ))
            });
        }
        if let Some((profile, fresh)) = verify {
            self.setup.launch(ctx, move |active| {
                let ready = pending::verify(&profile, &fresh, active)?;
                Ok(Answer::Verified(Box::new((profile, ready))))
            });
        }
    }
}
