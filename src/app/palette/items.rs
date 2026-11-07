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
        Self::new(label, hint, Entry::App(command))
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
    pub(super) fn palette_entries(&self) -> Vec<Item> {
        let mut list = vec![Item::app("New tab", "app", AppCommand::NewTab).shortcut(&crate::keymap::label(crate::keymap::Command::NewTab))];
        if !self.closed.is_empty() {
            list.push(
                Item::app("Reopen closed tab", "app", AppCommand::ReopenTab)
                    .shortcut(&crate::keymap::label(crate::keymap::Command::ReopenTab)),
            );
        }
        list.push(Item::app("Where's my work? (unpushed commits, stashes, changes everywhere)", "app", AppCommand::MyWork));
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
            Item::app("Settings", "app", AppCommand::Settings).shortcut(&crate::keymap::label(crate::keymap::Command::Settings)),
            Item::app("Manual and keyboard shortcuts", "app", AppCommand::Manual).shortcut(&crate::keymap::label(crate::keymap::Command::Manual)),
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
            list.push(Item::app("Teleport to another machine…", "tab", AppCommand::Teleport));
            let mut repo_items: Vec<Item> = repo
                .palette_items()
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
                item.label = format!("Check out {}", item.label);
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
