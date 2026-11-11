//! Portable setup preview and background handoffs. Imported paths never choose destinations.
mod view;

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
fn author_rule_has_url_scheme(host: &str) -> bool {
    let Some((scheme, _)) = host.trim().split_once("://") else {
        return false;
    };
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// Diagnose likely rule typos without rewriting imported rules or rejecting custom hosts.
fn author_rule_has_suspicious_syntax(rule: &str) -> bool {
    let (host, namespace) = rule.trim().split_once('/').unwrap_or((rule.trim(), ""));
    let bad_namespace =
        namespace.trim_end_matches('/').starts_with('/') || namespace.contains(['?', '#']);
    let bad_port = !host.starts_with('[')
        && host.bytes().filter(|b| *b == b':').count() == 1
        && host
            .split_once(':')
            .is_some_and(|(_, port)| !port.is_empty() && !port.bytes().all(|b| b.is_ascii_digit()));
    bad_namespace || bad_port
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
        for rule in &p.author_hosts {
            let warning = if author_rule_has_url_scheme(&rule.host) {
                Some("interaction-setup-author-rule-url")
            } else if author_rule_has_suspicious_syntax(&rule.host) {
                Some("interaction-setup-author-rule-syntax")
            } else {
                None
            };
            if let Some(warning) = warning {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    crate::i18n::message(warning, &[("host", rule.host.as_str().into())]),
                );
            }
        }
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
}
