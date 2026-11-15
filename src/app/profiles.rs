//! Switching profiles: each has its own tabs, recent repositories and theme, kept here while
//! another one is in use, and its own settings (`settings::profiles`).

mod view;
#[cfg(test)]
use view::{author_line, profile_row};

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{RichText, Sense, Ui, vec2};
use serde::{Deserialize, Serialize};

use super::panels::Toast;
use super::{App, MAX_RECENT, Tab, active_among_kept};
use crate::app_buttons::{self, AppButton};
use crate::repo::RepoTab;
use crate::settings::{self, ProfileRequest};
use crate::theme::{self, ThemeId, Tokens};

/// What a profile not in use had open, to open again when it is switched back to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ProfileState {
    pub open: Vec<PathBuf>,
    /// The active tab among `open`.
    pub active: usize,
    pub recent: Vec<PathBuf>,
    pub theme: ThemeId,
}

/// The profiles not in use, by id.
pub(super) type ProfileStates = BTreeMap<u64, ProfileState>;

/// How the tab strip calls a tab's unfinished work (see `worktree_tab::Unfinished`).
fn unfinished_phrase(label: &str) -> &str {
    match label {
        "running" => crate::i18n::text("interaction-tabs-running"),
        "terminal" => crate::i18n::text("interaction-tabs-terminal"),
        "commit message" => crate::i18n::text("interaction-tabs-message"),
        "editor" => crate::i18n::text("interaction-tabs-editor"),
        "review draft" => crate::i18n::text("interaction-tabs-review"),
        other => other,
    }
}

impl App {
    /// The repository tabs open, and which is active among them.
    pub(super) fn open_repos(&self) -> (Vec<PathBuf>, usize) {
        let open = self
            .tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
            .filter_map(|t| match t {
                Tab::Repo(r) => Some(r.path.clone()),
                Tab::Home | Tab::Settings => None,
            })
            .collect();
        let active = self
            .tabs
            .iter()
            .take(self.active + 1)
            .filter(|t| matches!(t, Tab::Repo(_)))
            .count()
            .saturating_sub(1);
        (open, active)
    }

    /// What the profile in use has open now.
    fn current_state(&self) -> ProfileState {
        let (open, active) = self.open_repos();
        ProfileState {
            open,
            active,
            recent: self.recent.clone(),
            theme: self.theme.clone(),
        }
    }

    /// Opens what `state` had open in place of the tabs there are. Repositories are read once
    /// shown, as on start.
    fn restore_state(&mut self, state: ProfileState, ctx: &egui::Context) {
        let settings_open = self.tabs.iter().any(|t| matches!(t, Tab::Settings));
        let settings_active = matches!(self.tabs.get(self.active), Some(Tab::Settings));
        self.save_all_tabs();
        // The root view is temporarily parked during a deferred callback; retain its slot
        // for the callback to restore, then let it install the new profile in the root.
        self.windows
            .parked
            .retain(|id, _| *id == egui::ViewportId::ROOT);
        self.tabs.clear();
        self.tab_drag = None;
        self.closed.clear();
        let mut kept = Vec::new();
        for path in state.open {
            let is_dir = super::tabs::reopenable(&path);
            kept.push(is_dir);
            if is_dir {
                self.tabs
                    .push(Tab::Repo(Box::new(RepoTab::new_deferred(path, ctx))));
            }
        }
        if self.tabs.is_empty() {
            self.tabs.push(Tab::Home);
        }
        self.active = active_among_kept(&kept, state.active).min(self.tabs.len() - 1);
        // The settings page stays where it was being used, now for the other profile.
        if settings_open {
            self.tabs.push(Tab::Settings);
            if settings_active {
                self.active = self.tabs.len() - 1;
            }
        }
        self.recent = state.recent;
        self.recent.truncate(MAX_RECENT);
        self.set_theme(ctx, state.theme);
    }

    /// Whether the profile can be switched now. Not while a tab has work that closing it would
    /// lose, nor while a repository is being cloned or connected to (it would open in the other
    /// profile): a toast says why.
    fn can_switch_profile(&mut self) -> bool {
        if self.setup.busy()
            || self.practicing.is_some()
            || self.cloning.is_some()
            || !self.connecting.is_empty()
        {
            self.toasts.push(Toast::new(
                crate::i18n::text("interaction-tabs-wait").to_owned(),
                true,
            ));
            return false;
        }
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
        {
            if let Tab::Repo(r) = tab
                && let Some(work) = r.unfinished_anywhere()
            {
                let name = r.label(true).repo;
                self.toasts.push(Toast::new(
                    crate::i18n::message(
                        "interaction-tabs-unfinished",
                        &[
                            ("name", name.into()),
                            ("work", unfinished_phrase(work).into()),
                        ],
                    ),
                    true,
                ));
                return false;
            }
        }
        true
    }

    /// Switches to profile `id`: its settings, tabs and theme. Refused, with a toast, while a
    /// tab has work that closing it would lose. Returns whether it switched.
    pub(super) fn switch_profile(&mut self, id: u64, ctx: &egui::Context) -> bool {
        let Some(next) = self
            .settings
            .user_profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return false;
        };
        if next.pending_setup() {
            self.toasts.push(Toast::new(
                crate::i18n::text("interaction-tabs-setup").into(),
                true,
            ));
            return false;
        }
        if id == self.settings.active_profile {
            return true;
        }
        if !self.can_switch_profile() {
            return false;
        }
        self.invalidate_repository_contexts();
        let current = self.current_state();
        self.profile_states
            .insert(self.settings.active_profile, current);
        let state = self
            .profile_states
            .remove(&id)
            .unwrap_or_else(|| ProfileState {
                // A new profile starts with the look of the one it was made from.
                theme: self.theme.clone(),
                ..ProfileState::default()
            });
        self.settings.active_profile = id;
        self.library.switch_profile();
        self.settings.set(ctx);
        // The tabs opened next read the new profile's author themselves; a later change to it
        // is noticed against what it is now.
        self.profile_git_seen = settings::profiles::config_text(&next, &[]);
        self.restore_state(state, ctx);
        // The inbox starts over: a pull request waiting to open would wait on a tab that's
        // gone. What was seen of each stays, being the forge account's, not the profile's.
        self.inbox = Default::default();
        self.toasts.push(Toast::new(
            crate::i18n::message(
                "interaction-tabs-switched",
                &[("profile", next.display_title().into())],
            ),
            false,
        ));
        true
    }

    fn invalidate_repository_contexts(&mut self) {
        for tab in self.tabs.iter_mut().chain(
            self.windows
                .parked
                .values_mut()
                .flat_map(|window| &mut window.tabs),
        ) {
            if let Tab::Repo(repo) = tab {
                repo.invalidate_repository_context();
            }
        }
    }

    /// Adds a profile, a copy of the one in use when `copy`, and switches to it.
    fn new_profile(&mut self, copy: bool, ctx: &egui::Context) {
        let base = if copy {
            format!("{} copy", self.settings.profile().title())
        } else {
            "New profile".to_owned()
        };
        // Refused before it is added, so a refused switch leaves no profile behind.
        if !self.can_switch_profile() {
            return;
        }
        let name = self.settings.new_profile_name(&base);
        let id = self.settings.add_profile(&name, copy);
        self.settings.set(ctx);
        self.switch_profile(id, ctx);
    }

    /// Deletes profile `id`, switching to another first when it is in use.
    fn delete_profile(&mut self, id: u64, ctx: &egui::Context) {
        if self.settings.user_profiles.len() < 2 {
            return;
        }
        if id == self.settings.active_profile {
            let Some(other) = self
                .settings
                .user_profiles
                .iter()
                .find(|p| p.id != id)
                .map(|p| p.id)
            else {
                return;
            };
            if !self.switch_profile(other, ctx) {
                return;
            }
        }
        self.setup.forget_profile(id);
        self.settings.user_profiles.retain(|p| p.id != id);
        self.profile_states.remove(&id);
        self.library.forget_profile(id);
        settings::profiles::forget(id);
        self.settings.set(ctx);
    }

    pub(super) fn profile_request(&mut self, request: ProfileRequest, ctx: &egui::Context) {
        match request {
            ProfileRequest::Switch(id) => {
                self.switch_profile(id, ctx);
            }
            ProfileRequest::New => self.new_profile(false, ctx),
            ProfileRequest::Duplicate => self.new_profile(true, ctx),
            ProfileRequest::Delete(id) => self.delete_profile(id, ctx),
            ProfileRequest::Theme(id) => self.set_theme(ctx, id),
            ProfileRequest::RepositoryRoots => self.library_open = true,
        }
    }

    /// Once back on a repository after the profile's author or signing changed, the repository
    /// tabs read again who commits are by. Not while the settings are being typed in.
    pub(super) fn notice_profile_git(&mut self, ctx: &egui::Context) {
        if !matches!(self.tabs.get(self.active), Some(Tab::Repo(_))) {
            return;
        }
        let profile = self.settings.profile();
        let now = settings::profiles::config_text(&profile, &[]);
        if self.profile_git_seen.is_empty() {
            self.profile_git_seen = now;
            return;
        }
        if self.profile_git_seen != now {
            self.invalidate_repository_contexts();
            self.profile_git_seen = now;
            self.inbox = Default::default();
            for tab in self
                .tabs
                .iter_mut()
                .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
            {
                // One not read yet, or suspended, reads it when shown.
                if let Tab::Repo(r) = tab {
                    r.notice_connections(ctx);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "profiles/localization_tests.rs"]
mod localization_tests;
