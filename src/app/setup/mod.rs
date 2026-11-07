//! Portable setup preview and background handoffs. Imported paths never choose destinations.
mod files;
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
}
struct Job {
    rx: Receiver<Result<Answer, String>>,
    active: Arc<AtomicBool>,
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
        crate::connections::spawn(move || {
            let result = work(&|| worker.load(Ordering::Acquire));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.job = Some(Job { rx, active });
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
    fn set_file(&mut self, file: Setup) {
        self.plans = file
            .repositories
            .iter()
            .map(|r| Plan {
                name: r.name.clone(),
                ..Default::default()
            })
            .collect();
        self.file = Some(Arc::new(file));
        self.mappings.clear();
        self.applied = false;
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
                    if let (Some(receipt), Some(file)) = (&plan.receipt, &self.setup.file) {
                        if let Some(r) = file.repositories.get(index) {
                            crate::repo::RepoTab::restore_portable_visibility(
                                ctx,
                                &receipt.root,
                                &r.hidden,
                            );
                        }
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
            ui.heading("New machine, same setup");
            ui.label("Retries remain available during this app session while the owned clone/config is unchanged. Quitting retains files; finish them manually or choose a fresh destination. LFS content and submodules need separate setup after checkout.");
            ui.label("One versioned setup file carries public settings and repository descriptions. It contains no credentials, key bytes, authentication folders or executable configuration. Old paths are labels only; working files, stashes and local-only objects need a separate backup.");
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()*0.72).max(150.0)).show(ui,|ui|{
                ui.add_enabled_ui(!busy,|ui|{ui.horizontal(|ui|{ui.selectable_value(&mut self.setup.import,false,"Export setup");ui.selectable_value(&mut self.setup.import,true,"Import setup");});
                    if !self.setup.import {
                        ui.label("Select recent/open repositories to include. Metadata reads run in the background on each repository's own machine. Cloning from an imported file always targets this machine.");
                        for candidate in &mut self.setup.candidates {ui.checkbox(&mut candidate.selected,candidate.path.display().to_string());}
                        export=ui.button("Build export preview").clicked();
                    }else{load=ui.button("Load setup file…").clicked();}
                });
                if let Some(file)=self.setup.file.clone(){
                    ui.separator();ui.label(format!("Preview: {} profiles, {} saved authors, {} repositories",file.profiles.len(),file.identities.len(),file.repositories.len()));
                    for p in &file.profiles {ui.label(format!("Profile: {} — {} <{}>{}",p.name,p.author_name,p.author_email,if p.setup.pending(){" · local authentication/key setup required"}else{""}));for a in &p.setup.accounts{ui.label(format!("  {:?}: {} on {}",a.provider,a.user_or_alias,a.host));}}
                    ui.add_enabled_ui(!busy,|ui|{if self.setup.import{ui.checkbox(&mut self.setup.preferences,"Restore public display settings");let response=ui.add_enabled(!self.setup.applied,egui::Button::new("Add profiles, authors and public settings"));
                        apply=response.clicked();}else{save=ui.button("Save setup file…").clicked();}});
                    for (index,r) in file.repositories.iter().enumerate(){
                        ui.push_id(index,|ui|{ui.group(|ui|{
                            ui.label(format!("{} — previous location: {}",r.name,r.previous_path));
                            ui.label(format!("Checkout: {} · {} linked worktrees · {} hidden branches",r.branch.as_deref().unwrap_or("detached"),r.worktrees.len(),r.hidden.len()));
                            for remote in &r.remotes {ui.label(format!("{}: {}",remote.name,remote.fetch));for url in &remote.push{ui.label(format!("Push: {url}"));}}
                            for w in &r.worktrees{ui.label(format!("Worktree {}: {} at {} ({})",w.name,w.branch.as_deref().unwrap_or("detached"),w.commit,w.previous_path));}
                            for notice in &r.notices {ui.label(notice);}
                            if self.setup.import {
                                let matching=r.profile_ids.iter().filter_map(|id|self.setup.mappings.get(id)).copied().collect::<Vec<_>>();
                                let permitted=self.setup.applied && (r.profile_ids.is_empty() || matching.contains(&self.settings.active_profile)) && !self.settings.profile().pending_setup();
                                let plan=&mut self.setup.plans[index];
                                ui.add_enabled_ui(!busy,|ui|{
                                    ui.checkbox(&mut plan.selected,"Select this repository");
                                    ui.add_enabled_ui(plan.receipt.is_none(),|ui|{
                                        ui.horizontal(|ui|{ui.label("New folder name");ui.text_edit_singleline(&mut plan.name);});
                                        if ui.button("Choose destination parent…").clicked(){if let Some(path)=rfd::FileDialog::new().pick_folder(){plan.parent=path;}}
                                        if ui.button("Use local source folder on this machine…").clicked(){if let Some(path)=rfd::FileDialog::new().pick_folder(){plan.source=Some(path);}}
                                        if plan.source.is_some() && ui.button("Use portable remote instead").clicked(){plan.source=None;}
                                    });
                                    ui.label(format!("New destination: {}",plan.parent.join(&plan.name).display()));
                                    if let Some(source)=&plan.source{ui.label(format!("Current-machine source: {}",source.display()));}
                                    if !r.worktrees.is_empty(){ui.label(format!("New worktree root: {}",plan.parent.join(format!("{}-worktrees",plan.name)).display()));}
                                    if !permitted {ui.label("Add the public profiles, finish their required setup, then explicitly activate a corresponding profile before cloning.");for id in &matching {if let Some(p)=self.settings.user_profiles.iter().find(|p|p.id==*id && !p.pending_setup()){if ui.button(format!("Activate {}",p.title())).clicked(){activate=Some(*id);}}}}
                                    if ui.add_enabled(plan.selected && permitted && !plan.parent.as_os_str().is_empty() && !plan.complete,egui::Button::new(if plan.receipt.is_some(){"Retry remaining steps"}else{"Clone and restore selected setup"})).clicked(){restore=Some(index);}
                                    if plan.receipt.is_some() && ui.button("Choose a new destination instead").clicked(){plan.receipt=None;plan.complete=false;plan.parent=PathBuf::new();plan.name=format!("{}-retry",r.name);plan.messages.push("Previous clone/worktrees were retained. Choose a new unused destination.".into());}
                                    if plan.complete && ui.button("Open restored repository").clicked(){open=plan.receipt.as_ref().map(|r|r.root.clone());}
                                });
                                for message in &plan.messages{ui.label(message);}
                            }
                        });});
                    }
                }
                ui.separator();ui.heading("Finish profile setup");
                ui.label("Imported safety expectations remain inactive until all required local selections and account checks pass. No account is switched or signed in automatically.");
                for profile in self.settings.user_profiles.iter().filter(|p|p.pending_setup()) {ui.push_id(profile.id,|ui|{ui.group(|ui|{ui.label(profile.title());let fresh=self.setup.fresh.entry(profile.id).or_default();ui.add_enabled_ui(!busy,|ui|{if pending::fields(ui,profile,fresh){verify=Some((profile.clone(),fresh.clone()));}});});});}
                for message in &self.setup.messages{ui.label(message);}
            });
            if busy {ui.horizontal(|ui|{ui.spinner();ui.label("Working in the background…");if ui.button("Cancel setup operation").clicked(){if let Some(job)=&self.setup.job{job.active.store(false,Ordering::Release);}}});}
            close=ui.add_enabled(!busy,egui::Button::new("Close")).clicked();
        });
        if close || (!busy && modal.should_close()) {
            self.setup.open = false;
        }
        if load {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Gitcraft setup", &["json"])
                .pick_file()
            {
                self.setup.launch(ctx, move |active| {
                    files::load(&path, active).map(Answer::Loaded)
                });
            }
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
        if save {
            if let (Some(file), Some(path)) = (
                self.setup.file.clone(),
                rfd::FileDialog::new()
                    .set_file_name("gitcraft-setup.json")
                    .save_file(),
            ) {
                self.setup.launch(ctx, move |active| {
                    files::save(&path, &file, active)?;
                    Ok(Answer::Saved(path))
                });
            }
        }
        if apply {
            if let Err(e) = self.apply_setup_public(ctx) {
                self.setup.messages.push(e);
            }
        }
        if let Some(id) = activate {
            self.switch_profile(id, ctx);
        }
        if let Some(path) = open {
            self.open_root(path, ctx);
            self.setup.open = false;
        }
        if let Some(index) = restore {
            if let Some(file) = &self.setup.file {
                let plan = &self.setup.plans[index];
                let mut repo = file.repositories[index].clone();
                repo.name = plan.name.clone();
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
        }
        if let Some((profile, fresh)) = verify {
            self.setup.launch(ctx, move |active| {
                let ready = pending::verify(&profile, &fresh, active)?;
                Ok(Answer::Verified(Box::new((profile, ready))))
            });
        }
    }
}
