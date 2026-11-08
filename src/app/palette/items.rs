//! The palette's entries: the app's commands, the open repository's, and what running one does.

use super::score::Folded;
use crate::app::{App, Tab};
use crate::repo::{Action, PaletteItem, PaletteRun, Pick};
use crate::theme::{self, ThemeId};

/// One palette entry, folded once for matching.
pub(crate) struct Item {
    pub label: String,
    pub folded: Folded,
    /// Its length in characters: of two that match as well, the shorter goes first.
    pub len: usize,
    pub hint: String,
    pub shortcut: Option<String>,
    /// What the recently used list knows it by.
    pub key: String,
    /// About the selected commit or file: first when nothing is typed.
    pub selection: bool,
    /// Listed only once something is typed, and only the few that match best (checking out
    /// any branch from the first step, without listing thousands of them).
    pub typed_only: bool,
    pub run: Entry,
}

/// What a palette entry does outside the open repository.
pub(crate) enum AppCommand {
    NewTab,
    ReopenTab,
    Open,
    Create,
    Clone,
    OpenRemote,
    Teleport,
    ForgetSignIns,
    Settings,
    Manual,
    CopyDiagnostics,
    MyWork,
    Theme(ThemeId),
    /// The settings' themes.
    Themes,
    Profile(u64),
    NewProfile,
}

pub(crate) enum Entry {
    App(AppCommand),
    /// One of the active repository's actions.
    Repo(Action),
    /// Lists the branches (or modes) to do it with, in a second step.
    Pick(Pick),
}

/// Resolve a presentation label without changing the existing persisted recent key.
fn app_label(raw: &str, command: &AppCommand) -> String {
    let id = match command {
        AppCommand::NewTab => "interaction-palette-new-tab",
        AppCommand::ReopenTab => "interaction-palette-reopen",
        AppCommand::Open => "interaction-palette-open",
        AppCommand::Create => "interaction-palette-create",
        AppCommand::Clone => "interaction-palette-clone",
        AppCommand::OpenRemote => "interaction-palette-remote",
        AppCommand::Teleport => "interaction-palette-teleport",
        AppCommand::ForgetSignIns => "interaction-palette-forget",
        AppCommand::Settings => "interaction-palette-settings",
        AppCommand::Manual => "interaction-palette-manual",
        AppCommand::CopyDiagnostics => "interaction-palette-diagnostics",
        AppCommand::MyWork => "interaction-palette-my-work",
        AppCommand::Themes => "interaction-palette-themes",
        AppCommand::NewProfile => "interaction-palette-new-profile",
        AppCommand::Theme(_) => {
            return crate::i18n::message(
                "interaction-palette-theme",
                &[("theme", raw.strip_prefix("Theme: ").unwrap_or(raw).into())],
            );
        }
        AppCommand::Profile(_) => {
            return crate::i18n::message(
                "interaction-palette-profile",
                &[(
                    "profile",
                    raw.strip_prefix("Switch to profile: ")
                        .unwrap_or(raw)
                        .into(),
                )],
            );
        }
    };
    crate::i18n::text(id).to_owned()
}

fn app_hint(raw: &str) -> &str {
    match raw {
        "app" => crate::i18n::text("interaction-palette-app"),
        "settings" => crate::i18n::text("interaction-palette-settings-kind"),
        "profile" => crate::i18n::text("interaction-palette-profile-kind"),
        "tab" => crate::i18n::text("interaction-palette-tab"),
        "dark theme" => crate::i18n::text("interaction-palette-dark"),
        "light theme" => crate::i18n::text("interaction-palette-light"),
        _ => raw,
    }
}

impl Item {
    pub(crate) fn new(label: impl Into<String>, hint: &str, run: Entry) -> Self {
        let label = label.into();
        Self {
            folded: Folded::new(&label),
            len: label.chars().count(),
            key: label.clone(),
            label,
            hint: hint.to_owned(),
            shortcut: None,
            selection: false,
            typed_only: false,
            run,
        }
    }

    fn app(label: &str, hint: &str, command: AppCommand) -> Self {
        let display = app_label(label, &command);
        let mut item = Self::new(display, app_hint(hint), Entry::App(command));
        item.key = label.to_owned();
        item
    }

    fn shortcut(mut self, keys: &str) -> Self {
        self.shortcut = Some(keys.to_owned());
        self
    }

    /// One of the repository's entries.
    pub(crate) fn from_repo(p: PaletteItem) -> Self {
        let run = match p.run {
            PaletteRun::Action(action) => Entry::Repo(action),
            PaletteRun::Pick(pick) => Entry::Pick(pick),
        };
        let mut item = Self::new(p.label, &p.hint, run);
        item.key = p.key;
        item.shortcut = p.shortcut;
        item.selection = p.selection;
        item
    }
}

impl App {
    /// Every entry of the palette's first step.
    pub(super) fn palette_entries(&self, ctx: &egui::Context) -> Vec<Item> {
        let mut list = vec![
            Item::app("New tab", "app", AppCommand::NewTab)
                .shortcut(&crate::keymap::label(ctx, crate::keymap::Command::NewTab)),
        ];
        if !self.closed.is_empty() {
            list.push(
                Item::app("Reopen closed tab", "app", AppCommand::ReopenTab).shortcut(
                    &crate::keymap::label(ctx, crate::keymap::Command::ReopenTab),
                ),
            );
        }
        list.push(Item::app(
            "Where's my work? (unpushed commits, stashes, changes everywhere)",
            "app",
            AppCommand::MyWork,
        ));
        list.push(Item::app(
            "Help: Copy diagnostics",
            "app",
            AppCommand::CopyDiagnostics,
        ));
        list.extend([
            Item::app("Open repository…", "app", AppCommand::Open),
            Item::app("Create repository…", "app", AppCommand::Create),
            Item::app("Clone repository…", "app", AppCommand::Clone),
            Item::app(
                "Open remote repository (WSL, SSH)…",
                "app",
                AppCommand::OpenRemote,
            ),
            Item::app(
                "Forget saved SSH sign-ins",
                "app",
                AppCommand::ForgetSignIns,
            ),
            Item::app("Settings", "app", AppCommand::Settings)
                .shortcut(&crate::keymap::label(ctx, crate::keymap::Command::Settings)),
            Item::app("Manual and keyboard shortcuts", "app", AppCommand::Manual)
                .shortcut(&crate::keymap::label(ctx, crate::keymap::Command::Manual)),
        ]);
        for t in theme::themes().iter() {
            let hint = if t.dark() {
                "dark theme"
            } else {
                "light theme"
            };
            list.push(Item::app(
                &format!("Theme: {}", t.name),
                hint,
                AppCommand::Theme(t.id.clone()),
            ));
        }
        list.push(Item::app("Browse themes…", "settings", AppCommand::Themes));
        for p in &self.settings.user_profiles {
            if p.id != self.settings.active_profile {
                list.push(Item::app(
                    &format!("Switch to profile: {}", p.title()),
                    "profile",
                    AppCommand::Profile(p.id),
                ));
            }
        }
        list.push(Item::app("New profile", "profile", AppCommand::NewProfile));
        if let Some(Tab::Repo(repo)) = self.tabs.get(self.active) {
            list.push(Item::app(
                "Teleport to another machine…",
                "tab",
                AppCommand::Teleport,
            ));
            let mut repo_items: Vec<Item> = repo
                .palette_items(ctx)
                .into_iter()
                .map(Item::from_repo)
                .collect();
            // What is selected goes before the app's commands.
            let selected = repo_items.iter().take_while(|i| i.selection).count();
            let rest = repo_items.split_off(selected);
            list.splice(0..0, repo_items);
            list.extend(rest);
            list.extend(repo.palette_choices(&Pick::Checkout).into_iter().map(|p| {
                let mut item = Item::from_repo(p);
                item.label = crate::i18n::message(
                    "interaction-palette-checkout",
                    &[("branch", item.label.as_str().into())],
                );
                item.folded = Folded::new(&item.label);
                item.len = item.label.chars().count();
                item.typed_only = true;
                item
            }));
        }
        list
    }

    /// The second step's entries for `pick`, in the active repository.
    pub(super) fn palette_choices(&self, pick: &Pick) -> Vec<Item> {
        match self.tabs.get(self.active) {
            Some(Tab::Repo(repo)) => repo
                .palette_choices(pick)
                .into_iter()
                .map(Item::from_repo)
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Runs an entry other than a pick (which opens the second step instead).
    pub(super) fn run_entry(&mut self, entry: Entry, ctx: &egui::Context) {
        let command = match entry {
            Entry::App(command) => command,
            Entry::Repo(action) => {
                if let Some(Tab::Repo(repo)) = self.tabs.get_mut(self.active) {
                    repo.queue(action);
                    ctx.request_repaint();
                }
                return;
            }
            Entry::Pick(_) => return,
        };
        match command {
            AppCommand::NewTab => self.new_tab(),
            AppCommand::ReopenTab => self.reopen_closed_tab(ctx),
            AppCommand::Open => self.pick_existing(ctx),
            AppCommand::Create => self.pick_new(ctx),
            AppCommand::Clone => self.start_clone(),
            AppCommand::OpenRemote => self.start_open_remote(ctx),
            AppCommand::Teleport => {
                if let Some(Tab::Repo(repo)) = self.tabs.get(self.active) {
                    self.start_teleport(repo.path.clone(), ctx);
                }
            }
            // The system's store can take a while (the Secret Service is another process).
            AppCommand::ForgetSignIns => {
                std::thread::spawn(crate::remote::askpass::forget_saved);
            }
            AppCommand::Settings => self.open_settings(),
            AppCommand::Manual => self.manual_open = true,
            AppCommand::CopyDiagnostics => self.copy_diagnostics(ctx),
            AppCommand::MyWork => self.open_my_work(ctx),
            AppCommand::Theme(id) => self.set_theme(ctx, id),
            AppCommand::Themes => {
                crate::settings::show_page(ctx, crate::settings::Page::Appearance);
                self.open_settings();
            }
            AppCommand::Profile(id) => {
                self.switch_profile(id, ctx);
            }
            AppCommand::NewProfile => {
                self.profile_request(crate::settings::ProfileRequest::New, ctx);
            }
        }
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_german_palette_ranks_and_activates_with_stable_recent_key_and_command() {
        crate::i18n::set_language("de");
        let item = Item::app("New tab", "app", AppCommand::NewTab);
        assert_eq!(item.key, "New tab");
        assert_eq!(item.label, "Neuer Tab");
        assert!(matches!(item.run, Entry::App(AppCommand::NewTab)));
        let mut recent = super::super::recent::Recent::default();
        recent.used(&item.key);
        assert_eq!(recent.rank("New tab"), Some(0));
        assert_eq!(recent.rank("Neuer Tab"), None);
        let key = (0, None, String::new(), crate::i18n::generation());
        let fonts = std::cell::Cell::new(false);
        let state_key = key.clone();
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (super::super::Palette, bool)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                if let super::super::Outcome::Choose(index) = state.0.show(ui.ctx(), &key, &recent)
                {
                    assert_eq!(index, 0);
                    assert!(matches!(
                        state.0.list()[index].run,
                        Entry::App(AppCommand::NewTab)
                    ));
                    state.1 = true;
                }
            },
            (
                super::super::Palette {
                    query: "tab".into(),
                    selected: 0,
                    entries: Some((state_key, vec![item])),
                    step: None,
                    shown: None,
                    scroll: 0.0,
                },
                false,
            ),
        );
        h.run();
        h.get_by_label("Neuer Tab, Anwendung").click();
        h.run();
        assert!(h.state().1);
        assert_eq!(h.state().0.list()[0].key, "New tab");
        crate::i18n::set_language("en-US");
        let english = Item::app("New tab", "app", AppCommand::NewTab);
        assert_eq!(english.key, "New tab");
        assert_eq!(english.label, "New tab");
    }
}
