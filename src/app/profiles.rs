//! Switching profiles: each has its own tabs, recent repositories and theme, kept here while
//! another one is in use, and its own settings (`settings::profiles`).

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

    /// The profile in use on the tab strip, a menu to switch to another.
    /// The profile in use, and its menu: the profiles, and `extras`, the title bar's buttons
    /// when it has no room for them.
    pub(super) fn profile_button(&mut self, ui: &mut Ui, t: &Tokens, extras: &[AppButton]) {
        let profile = self.settings.profile();
        let font = theme::medium(12.0);
        let name =
            ui.painter()
                .layout_no_wrap(profile.display_title().to_owned(), font, t.text_dim);
        let width = (name.size().x + 34.0).min(180.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
        crate::repo::widgets::a11y(&resp, egui::WidgetType::ComboBox, || {
            crate::i18n::message(
                "interaction-tabs-profile-a11y",
                &[("profile", profile.display_title().into())],
            )
        });
        if resp.hovered() {
            ui.painter().rect_filled(rect, t.radius, t.pressed);
        }
        let badge = egui::pos2(rect.left() + 15.0, rect.center().y);
        settings::paint_badge(ui, badge, 9.0, &profile);
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 28.0, rect.top()),
            egui::pos2(rect.right() - 6.0, rect.bottom()),
        );
        ui.painter().with_clip_rect(text_rect).galley(
            egui::pos2(text_rect.left(), rect.center().y - name.size().y / 2.0),
            name,
            t.text_dim,
        );
        let resp = resp.on_hover_text(crate::i18n::message(
            "interaction-tabs-profile-tip",
            &[
                ("profile", profile.display_title().into()),
                ("author", author_line(&profile).into()),
            ],
        ));
        let mut request = None;
        egui::Popup::menu(&resp)
            .style(theme::menu_style)
            .show(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("interaction-tabs-profiles"))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                );
                for p in &self.settings.user_profiles {
                    let active = p.id == profile.id;
                    let row = profile_row(ui, p, active, t);
                    let author = author_line(p);
                    let row = if author.is_empty() {
                        row
                    } else {
                        row.on_hover_text(author.trim_start_matches(" · "))
                    };
                    if row.clicked() && !active {
                        request = Some(ProfileRequest::Switch(p.id));
                    }
                }
                ui.separator();
                if ui
                    .button(crate::i18n::text("interaction-tabs-new"))
                    .clicked()
                {
                    request = Some(ProfileRequest::New);
                }
                if ui
                    .button(crate::i18n::text("interaction-tabs-manage"))
                    .clicked()
                {
                    settings::show_page(ui.ctx(), settings::Page::Profile);
                    self.open_settings();
                }
                if !extras.is_empty() {
                    ui.separator();
                    app_buttons::menu_entries(ui, extras);
                }
            });
        if let Some(request) = request {
            self.profile_request(request, ui.ctx());
        }
    }
}

/// A profile's row in the title bar's menu: its badge and name across the menu's width, a
/// check on the one in use.
fn profile_row(ui: &mut Ui, p: &settings::UserProfile, active: bool, t: &Tokens) -> egui::Response {
    let width = ui.available_width().clamp(180.0, 260.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    crate::repo::widgets::a11y_selected(
        &resp,
        || {
            crate::i18n::message(
                "interaction-tabs-profile-a11y",
                &[("profile", p.display_title().into())],
            )
        },
        active,
    );
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.pressed);
    }
    settings::paint_badge(ui, egui::pos2(rect.left() + 13.0, rect.center().y), 8.0, p);
    let color = if active { t.text } else { t.text_dim };
    let font = if active {
        theme::medium(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    let name = ui
        .painter()
        .layout_no_wrap(p.display_title().to_owned(), font, color);
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 28.0, rect.top()),
        egui::pos2(rect.right() - 26.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(text_rect).galley(
        egui::pos2(text_rect.left(), rect.center().y - name.size().y / 2.0),
        name,
        color,
    );
    if active {
        let check = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 13.0, rect.center().y),
            vec2(14.0, 14.0),
        );
        crate::icons::paint(ui, check, "check", 14.0, t.accent);
    }
    resp
}

/// " · Ann <ann@example.com>" for a profile that sets an author, "" otherwise.
fn author_line(p: &settings::UserProfile) -> String {
    let (name, email) = (p.author_name.trim(), p.author_email.trim());
    match (name.is_empty(), email.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!(" · {name}"),
        (true, false) => format!(" · <{email}>"),
        (false, false) => format!(" · {name} <{email}>"),
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_profile_row_keeps_identity_selection_and_opaque_author_across_languages()
    -> Result<(), String> {
        crate::i18n::set_language("de");
        let profile = settings::UserProfile {
            name: "Arbeitsprofil 日本語".into(),
            author_name: "Jonas 日本語".into(),
            author_email: "jonas@example.invalid".into(),
            ..Default::default()
        };
        let before = serde_json::to_string(&profile).map_err(|error| format!("{error:?}"))?;
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (settings::UserProfile, bool, Option<egui::Id>)| {
                if !fonts.replace(true) {
                    theme::install_fonts(ui.ctx());
                    return;
                }
                let response = profile_row(ui, &state.0, true, &Tokens::get(ui.ctx()));
                if let Some(id) = state.2 {
                    assert_eq!(response.id, id);
                }
                state.2 = Some(response.id);
                state.1 |= response.clicked();
            },
            (profile, false, None),
        );
        h.run();
        h.get_by_label("Profil Arbeitsprofil 日本語").click();
        h.run();
        assert!(h.state().1);
        assert_eq!(
            author_line(&h.state().0),
            " · Jonas 日本語 <jonas@example.invalid>"
        );
        assert_eq!(
            serde_json::to_string(&h.state().0).map_err(|error| format!("{error:?}"))?,
            before
        );
        crate::i18n::set_language("en-US");
        h.run();
        h.get_by_label("profile Arbeitsprofil 日本語");
        assert_eq!(
            serde_json::to_string(&h.state().0).map_err(|error| format!("{error:?}"))?,
            before
        );

        Ok(())
    }

    #[test]
    fn unfinished_work_translates_only_known_display_phrases() {
        crate::i18n::set_language("de");
        assert_eq!(unfinished_phrase("running"), "einen laufenden Vorgang");
        assert_eq!(unfinished_phrase("terminal"), "ein geöffnetes Terminal");
        assert_eq!(
            unfinished_phrase("commit message"),
            "eine geschriebene Commit-Nachricht"
        );
        assert_eq!(unfinished_phrase("editor"), "einen geöffneten Editor");
        assert_eq!(unfinished_phrase("review draft"), "einen Review-Entwurf");
        assert_eq!(
            unfinished_phrase("external operation 日本語"),
            "external operation 日本語"
        );
        crate::i18n::set_language("en-US");
        assert_eq!(unfinished_phrase("running"), "an operation running");
    }
}
