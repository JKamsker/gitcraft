//! Window shell: git check, browser-like tabs, the start page and cloning.

mod agent_card;
mod askpass_dialog;
mod clone;
mod diagnostics;
mod draft_saving;
mod dropped;
mod handoff;
mod home;
mod home_filter;
mod home_recent;
mod inbox;
mod launch;
mod links;
mod my_work;
mod os_shell;
mod palette;
mod panels;
mod profiles;
mod recent_status;
mod remote_open;
mod repo_scan;
mod tab_strip;
mod tabs;
mod teleport;
#[cfg(test)]
mod tests;
pub(crate) mod title_bar;
mod update_banner;
mod windows;
mod workspaces;
mod worktrees;
pub use windows::DesktopApp;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::Ui;
use serde::{Deserialize, Serialize};

use crate::git;
use crate::keymap;
use crate::repo::{OpenRequest, RepoTab};
use crate::settings::Settings;
use crate::tab_look::TabLook;
use crate::theme::{self, ThemeId, Tokens};
use crate::update;
use clone::CloneForm;
use palette::Palette;
use panels::Toast;
use remote_open::{Connecting, RemoteForm};
use tab_strip::TabDrag;

const MAX_RECENT: usize = 12;

/// Set by `main` when the window was closed maximized: it opens restored and is maximized once
/// it shows its first frame. Created maximized, winit shows the hidden window for an instant
/// (`ShowWindow(SW_MAXIMIZE)`) before anything is painted, a white flash on Windows.
pub static MAXIMIZE_WHEN_SHOWN: AtomicBool = AtomicBool::new(false);

/// What survives a restart.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    open: Vec<PathBuf>,
    active: usize,
    recent: Vec<PathBuf>,
    theme: ThemeId,
    clone_parent: Option<PathBuf>,
    settings: Settings,
    /// A release the user chose to skip.
    skipped_version: String,
    /// The version gone back to with "Go back to x.y.z": no update is offered while it is set.
    pinned_version: String,
    /// Repository tabs given their own icon or colour.
    tab_looks: BTreeMap<PathBuf, TabLook>,
    /// What the profiles not in use had open; `open`, `active`, `recent` and `theme` are the
    /// profile's in use.
    profile_states: profiles::ProfileStates,
    /// The command palette's entries run lately.
    palette_recent: palette::recent::Recent,
    /// When each recent repository was last opened, in seconds since 1970 (of every profile).
    last_opened: BTreeMap<PathBuf, i64>,
    /// Named groups of repositories, on the start page.
    workspaces: crate::across::workspaces::Workspaces,
    /// Repositories found on a machine by "Where's my work?".
    found_repos: Vec<PathBuf>,
    /// What was seen of each pull request (by URL) when it was opened from the inbox.
    inbox_seen: BTreeMap<String, crate::forge::inbox::Seen>,
    windows: Vec<windows::SavedWindow>,
}

enum Tab {
    /// The start page ("new tab"): open, create or clone a repository.
    Home,
    Repo(Box<RepoTab>),
    Settings,
}

pub struct App {
    windows: windows::Windows,
    git_version: Result<String, String>,
    tabs: Vec<Tab>,
    /// What is typed in the worktree switcher.
    switcher_query: String,
    /// The switcher just opened: its search field takes the keyboard.
    switcher_focus: bool,
    /// The worktree picked in the switcher, to open after the tab strip is drawn.
    switch_to: Option<PathBuf>,
    /// The active tab's worktree is to be closed, after the tab strip is drawn.
    close_worktree: bool,
    active: usize,
    recent: Vec<PathBuf>,
    theme: ThemeId,
    clone_parent: Option<PathBuf>,
    error: Option<String>,
    /// What the start page's error is, when it is a git failure gitcraft can explain (see
    /// [`crate::error_card`]).
    error_card: Option<crate::error_card::ErrorCard>,
    was_focused: bool,
    clone_form: Option<CloneForm>,
    /// A clone running in the background: (destination, result).
    cloning: Option<clone::Cloning>,
    /// The form of the running clone, back on screen if it needs credentials.
    cloning_form: Option<CloneForm>,
    /// Started to pick a folder to open (`--open`, the launcher's Open…): done on the first frame.
    pick_on_start: bool,
    /// The last clone failed (for the taskbar button, which turns red).
    clone_failed: bool,
    /// What the taskbar button shows.
    taskbar: crate::os_shell::Taskbar,
    /// The recent repositories the taskbar button's menu was made for.
    menu_for: Option<Vec<PathBuf>>,
    /// The dialog that opens a repository in WSL or over SSH.
    remote_form: Option<RemoteForm>,
    /// Repositories on other machines being reached, each tab to open once it answers.
    connecting: Vec<Connecting>,
    /// The teleport dialog, and the teleport it runs.
    teleport: Option<teleport::TeleportForm>,
    /// ssh's question being answered (a password, a new host key).
    askpass: askpass_dialog::AskpassState,
    /// A newer release than this build.
    update: Option<update::Found>,
    update_rx: Option<Receiver<Result<Option<update::Found>, String>>>,
    /// The settings the last look for a newer release went by (whether to look, the channel,
    /// whether to download): changing them looks again.
    update_looked: Option<(bool, update::Channel, bool)>,
    /// Why the last look for a newer version failed.
    update_look_error: Option<String>,
    /// A look for a newer version came back, found or not.
    update_checked: bool,
    /// The update banner was put off until the next start.
    update_later: bool,
    /// Why the update didn't go in place.
    update_error: Option<String>,
    skipped_version: String,
    /// See `Saved::pinned_version`.
    pinned_version: String,
    /// The version the last update replaced, kept to go back to.
    previous_version: Option<update::Previous>,
    tab_looks: BTreeMap<PathBuf, TabLook>,
    settings: Settings,
    /// The profiles not in use: their tabs, recent repositories and theme.
    profile_states: profiles::ProfileStates,
    /// What the profile set for git when the repository tabs last read who commits are by.
    profile_git_seen: String,
    manual_open: bool,
    /// The graphics adapter wgpu draws with, for diagnostics.
    adapter: Option<String>,
    /// Diagnostics being collected for the clipboard.
    diagnostics: Option<diagnostics::Collecting>,
    palette: Option<Palette>,
    palette_recent: palette::recent::Recent,
    toasts: Vec<Toast>,
    /// The folders of the last repository tabs closed, the last one at the end.
    closed: Vec<PathBuf>,
    tab_drag: Option<TabDrag>,
    /// When what the tabs have typed is saved.
    keeper: crate::drafts::Keeper,
    /// This run's marker, and whether the last run ended without closing.
    session: crate::drafts::Session,
    /// The start page's search of the recent repositories.
    recent_filter: home_filter::RecentFilter,
    /// The recent repositories' branches and changes, read in the background.
    recent_status: recent_status::RecentStatuses,
    /// See [`Saved::last_opened`].
    last_opened: BTreeMap<PathBuf, i64>,
    /// Patches dropped on repository tabs, still to be applied.
    patches: dropped::PatchQueue,
    /// _Scan a folder for repositories_, while its dialog is open.
    repo_scan: Option<repo_scan::RepoScan>,
    workspaces: crate::across::workspaces::Workspaces,
    /// How the workspaces' repositories stand, as last looked at.
    workspaces_view: workspaces::WorkspacesView,
    /// Repositories found on a machine by "Where's my work?", looked at with the others.
    found_repos: Vec<PathBuf>,
    my_work: my_work::MyWork,
    inbox: inbox::Inbox,
    inbox_seen: BTreeMap<String, crate::forge::inbox::Seen>,
    /// What gitcraft was started for, being carried out.
    launching: launch::Launching,
    /// A forge link typed or pasted into the start page.
    home_link: String,
    /// How far the tabs are scrolled along the strip.
    tab_scroll: f32,
    /// The active tab the strip last scrolled into view.
    tab_scrolled_to: Option<egui::Id>,
    /// The window has the system's title bar (see `title_bar`).
    decorated: bool,
    /// Windows' border was last asked shown (true) or hidden (see `title_bar::sync_border`).
    window_border: Option<bool>,
}

impl App {
    /// `open_arg`: a repository passed on the command line, opened (or focused) on start.
    pub fn new(cc: &eframe::CreationContext<'_>, open_arg: Option<PathBuf>) -> Self {
        Self::launched(cc, open_arg.map(crate::launch::Command::Open))
    }

    /// The window, carrying out `cmd` from the command line (`gitcraft log <file>`…): its
    /// relative paths are taken from the current folder.
    pub fn launched(cc: &eframe::CreationContext<'_>, cmd: Option<crate::launch::Command>) -> Self {
        Self::launched_using(cc, cmd, git::version)
    }

    fn launched_using(
        cc: &eframe::CreationContext<'_>,
        cmd: Option<crate::launch::Command>,
        version: impl FnOnce() -> Result<String, String>,
    ) -> Self {
        // The window's thread: it never waits for another machine (see `remote::ui_guard`).
        crate::remote::mark_ui_thread();
        // ssh's questions (passwords, new host keys) come to this window.
        crate::remote::askpass::set_repaint(cc.egui_ctx.clone());
        // So do later starts' requests (see `crate::instance`).
        crate::instance::set_repaint(cc.egui_ctx.clone());
        let saved: Saved = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        let ctx = &cc.egui_ctx;
        egui_extras::install_image_loaders(ctx);
        crate::image_fit::install(ctx);
        crate::image_cache::install(ctx);
        theme::install_fonts(ctx);
        let mut settings = saved.settings;
        theme::apply(ctx, &saved.theme, &settings.theme_tweaks);
        settings.ensure_profile();
        settings.set(ctx);
        // Before anything could reach the credential store.
        crate::portable::set_credential_store(settings.portable_credential_store);
        crate::desktop_entry::refresh();

        let mut app = Self {
            windows: Default::default(),
            git_version: version(),
            tabs: Vec::new(),
            switcher_query: String::new(),
            switcher_focus: false,
            switch_to: None,
            close_worktree: false,
            active: 0,
            recent: saved.recent,
            theme: saved.theme.clone(),
            clone_parent: saved.clone_parent,
            settings,
            profile_states: saved.profile_states,
            profile_git_seen: String::new(),
            manual_open: false,
            adapter: diagnostics::adapter(cc),
            diagnostics: None,
            palette: None,
            palette_recent: saved.palette_recent,
            toasts: Vec::new(),
            closed: Vec::new(),
            tab_drag: None,
            keeper: Default::default(),
            session: crate::drafts::Session::start(),
            recent_filter: Default::default(),
            recent_status: Default::default(),
            last_opened: saved.last_opened,
            patches: Default::default(),
            repo_scan: None,
            launching: Default::default(),
            home_link: String::new(),
            tab_scroll: 0.0,
            tab_scrolled_to: None,
            decorated: title_bar::decorated_now(),
            window_border: None,
            error: None,
            error_card: None,
            was_focused: true,
            clone_form: None,
            cloning: None,
            cloning_form: None,
            pick_on_start: false,
            clone_failed: false,
            taskbar: Default::default(),
            menu_for: None,
            remote_form: None,
            connecting: Vec::new(),
            teleport: None,
            askpass: Default::default(),
            update: None,
            update_rx: None,
            update_look_error: None,
            update_checked: false,
            update_looked: None,
            update_later: false,
            update_error: None,
            skipped_version: saved.skipped_version,
            pinned_version: update::settle_pin(&saved.pinned_version, update::CURRENT),
            previous_version: update::previous(),
            tab_looks: saved.tab_looks,
            workspaces: saved.workspaces,
            workspaces_view: Default::default(),
            found_repos: saved.found_repos,
            my_work: Default::default(),
            inbox: Default::default(),
            inbox_seen: saved.inbox_seen,
        };
        // Before the window first shows, so its title bar doesn't flash.
        title_bar::apply(ctx, &app.settings, &mut app.decorated);
        update::clean_up();
        if let crate::portable::Mode::Unusable(why) = crate::portable::mode() {
            app.toasts.push(Toast::new(why.clone(), true));
        }
        let mut kept = Vec::new();
        if app.git_version.is_ok() {
            for path in saved.open {
                let is_dir = tabs::reopenable(&path);
                kept.push(is_dir);
                if is_dir {
                    // Read once shown, or once the one shown is read: one at a time first.
                    app.tabs
                        .push(Tab::Repo(Box::new(RepoTab::new_deferred(path, ctx))));
                }
            }
        }
        if app.tabs.is_empty() {
            app.tabs.push(Tab::Home);
        }
        app.restore_windows(saved.windows, ctx);
        app.active = active_among_kept(&kept, saved.active).min(app.tabs.len() - 1);
        if let Some(cmd) = cmd
            && app.git_version.is_ok()
        {
            let cwd = std::env::current_dir().unwrap_or_default();
            app.launch(cmd, cwd, ctx);
        }
        app
    }

    /// Opens `path` as the start page's "Open" button does (for `examples/ui_shot.rs`).
    pub fn open_tab(&mut self, path: &Path, ctx: &egui::Context) {
        self.open_repo(path, ctx);
    }

    /// Opens `path` in the active tab if it's a start page, otherwise in a new tab. One on
    /// another machine opens once it is reached, in the background.
    fn open_repo(&mut self, path: &Path, ctx: &egui::Context) {
        if self.focus_elsewhere(path, ctx) {
            return;
        }
        if crate::remote::is_remote(path) {
            // Already open: no need to reach it first.
            if let Some(i) = self
                .tabs
                .iter()
                .position(|t| matches!(t, Tab::Repo(r) if r.path == path))
            {
                self.remember(path);
                self.active = i;
                return;
            }
            self.connect(path.to_owned(), None, ctx);
            return;
        }
        match repo_root(path) {
            Ok(root) => self.open_root(root, ctx),
            Err(e) => {
                let old = self.error_card.take();
                self.error_card = match old {
                    // The same again: the card stays as it is.
                    Some(mut card) if card.is_about(&e) && card.dir() == path => {
                        card.failed_again();
                        Some(card)
                    }
                    old => match crate::error_card::ErrorCard::for_error(&e, path, "Open", ctx) {
                        Some(card) => Some(card.replacing(old)),
                        // A fix made stays undoable.
                        None => old.filter(|card| card.fixed()),
                    },
                };
                self.report(e);
            }
        }
    }

    /// Says why a repository couldn't be opened: on the start page, and in a toast when
    /// another tab is shown (where the start page's message would go unseen).
    fn report(&mut self, e: String) {
        if !matches!(self.tabs.get(self.active), Some(Tab::Home)) {
            self.toasts.push(Toast::new(e.clone(), true));
        }
        self.error = Some(e);
    }

    /// Opens the repository whose top folder is `root` (see [`Self::open_repo`]).
    fn open_root(&mut self, root: PathBuf, ctx: &egui::Context) {
        if self.focus_elsewhere(&root, ctx) {
            return;
        }
        self.remember(&root);
        self.error = None;
        self.error_card.take_if(|card| !card.fixed());

        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| matches!(t, Tab::Repo(r) if r.path == root))
        {
            self.active = i;
            return;
        }
        // A worktree parked in some tab comes out with everything it had.
        let parked = self.take_parked(&root);
        let came_back = parked.is_some();
        let mut view = parked.unwrap_or_else(|| Box::new(RepoTab::new(root, ctx)));
        if came_back {
            view.resume(ctx);
        }
        let tab = Tab::Repo(view);
        if matches!(self.tabs.get(self.active), Some(Tab::Home)) {
            self.tabs[self.active] = tab;
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len() - 1;
        }
    }

    fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MAX_RECENT);
        self.last_opened
            .insert(path.to_owned(), home_recent::now_secs());
        // Only what some profile's recent list still has.
        let kept: Vec<&PathBuf> = self
            .recent
            .iter()
            .chain(self.profile_states.values().flat_map(|s| &s.recent))
            .collect();
        self.last_opened.retain(|p, _| kept.contains(&p));
    }

    /// Carries out what was done with the app's buttons this frame, wherever they are.
    fn app_button_events(&mut self, ctx: &egui::Context) {
        use crate::app_buttons::{AppButton, Event};
        use crate::settings::ThemePick;
        for event in crate::app_buttons::take_events(ctx) {
            match event {
                Event::Clicked(AppButton::Settings) => self.open_settings(),
                Event::Clicked(AppButton::Palette) => self.open_palette(),
                Event::Clicked(AppButton::Manual) => self.manual_open = true,
                // The theme button opens its menu rather than being clicked.
                Event::Clicked(AppButton::Theme) => {}
                Event::Theme(ThemePick::Theme(id)) => self.set_theme(ctx, id),
                Event::Theme(ThemePick::Browse) => {
                    crate::settings::show_page(ctx, crate::settings::Page::Appearance);
                    self.open_settings();
                }
                Event::Move(button, place) => {
                    let layout = self.settings.button_layout.normalized();
                    self.settings.button_layout = layout.moved(button, place, usize::MAX);
                    self.settings.set(ctx);
                }
            }
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, id: ThemeId) {
        self.theme = id;
        self.apply_theme(ctx);
    }

    /// Paints the app with its theme and the settings' tweaks of it.
    fn apply_theme(&self, ctx: &egui::Context) {
        theme::apply(ctx, &self.theme, &self.settings.theme_tweaks);
    }

    /// Looks for a newer release once on start, and again when the update settings change;
    /// takes in what was found.
    fn poll_update(&mut self, ctx: &egui::Context) {
        let s = &self.settings;
        let want = (s.check_updates, s.update_channel, s.download_updates);
        // Offline or metered: looked for once that is over.
        if self.update_looked != Some(want) && crate::network::background_may_run() {
            let pinned = self.pinned_version.clone();
            self.update_looked = Some(want);
            self.update = None;
            self.update_rx = None;
            let (check, releases, download) = want;
            if check {
                let (tx, rx) = channel();
                let ctx = ctx.clone();
                let skipped = self.skipped_version.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(update::look(releases, download, &skipped, &pinned));
                    ctx.request_repaint();
                });
                self.update_rx = Some(rx);
            }
        }
        let Some(rx) = &self.update_rx else { return };
        let Ok(result) = rx.try_recv() else { return };
        self.update_rx = None;
        // A failed look (offline…) is tried again next start, or with "Check for updates".
        match result {
            Ok(found) => {
                self.update = found;
                self.update_checked = true;
                self.update_look_error = None;
            }
            Err(e) => self.update_look_error = Some(e),
        }
    }

    /// Looks for a newer version now ("Check for updates" in the settings), whether or not
    /// looking on start is on, and also for a version the user chose to skip.
    pub(super) fn check_update_now(&mut self, ctx: &egui::Context) {
        if self.update_rx.is_some() {
            return;
        }
        let s = &self.settings;
        let (releases, download) = (s.update_channel, s.download_updates);
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let pinned = self.pinned_version.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::look(releases, download, "", &pinned));
            ctx.request_repaint();
        });
        self.update_rx = Some(rx);
        self.update_look_error = None;
        // Asked for: the bar shows what is found, even after "Not now".
        self.update_later = false;
    }

    /// Where the look for a newer version stands, for the settings page.
    pub(super) fn update_check(&self) -> crate::settings::UpdateCheck {
        use crate::settings::UpdateCheck;
        if !self.pinned_version.is_empty() {
            UpdateCheck::Pinned(self.pinned_version.clone())
        } else if self.update_rx.is_some() {
            UpdateCheck::Checking
        } else if let Some(found) = &self.update {
            UpdateCheck::Found(found.release.version.clone())
        } else if let Some(e) = &self.update_look_error {
            UpdateCheck::Failed(format!("Couldn't look: {e}"))
        } else if self.update_checked {
            UpdateCheck::UpToDate
        } else {
            UpdateCheck::NotYet
        }
    }

    /// Keyboard shortcuts, and the reload on coming back to the window. Returns whether the
    /// window has the focus.
    fn shortcuts(&mut self, ctx: &egui::Context) -> bool {
        keymap::owner(
            ctx,
            egui::Id::new((
                self.active,
                self.tabs.get(self.active).and_then(|t| {
                    if let Tab::Repo(r) = t {
                        Some(&r.path)
                    } else {
                        None
                    }
                }),
            )),
        );
        keymap::active(
            ctx,
            match self.tabs.get(self.active) {
                Some(Tab::Repo(r)) => r.key_scopes(ctx),
                Some(Tab::Settings) => vec![keymap::Scope::Window, keymap::Scope::Settings],
                _ => vec![keymap::Scope::Window],
            },
        );
        // A focused text field or the terminal keeps these keys for itself.
        let modal_open = self.window_has_dialog()
            || self.askpass_open()
            || self.palette.is_some()
            || self.manual_open
            || self.repo_scan.is_some()
            || (crate::a11y::keyboard_taken(ctx) && !home_filter::search_has_focus(ctx));
        let focused =
            ctx.input(|i| i.focused || i.raw.viewports.values().any(|v| v.focused == Some(true)));
        let capturing = keymap::editor::capturing(ctx);
        let allowed = if capturing {
            Some(Vec::new())
        } else if modal_open {
            let mut allowed = vec![keymap::Command::Manual, keymap::Command::Palette];
            if self.palette.is_none()
                && !self.manual_open
                && self.clone_form.is_none()
                && self.remote_form.is_none()
                && !self.askpass_open()
                && let Some(Tab::Repo(r)) = self.tabs.get(self.active)
                && r.composer_has_focus(ctx)
            {
                allowed.push(keymap::Command::Commit);
            }
            if self.palette.is_none()
                && !self.manual_open
                && !self.window_has_dialog()
                && !self.askpass_open()
                && let Some(Tab::Repo(repo)) = self.tabs.get(self.active)
                && repo.search_shortcut_exception(ctx)
            {
                allowed.push(keymap::Command::Search);
            }
            if self.palette.is_none()
                && !self.manual_open
                && !self.window_has_dialog()
                && !self.askpass_open()
                && matches!(self.tabs.get(self.active), Some(Tab::Settings))
                && Settings::search_has_focus(ctx)
            {
                allowed.extend([
                    keymap::Command::SettingsSearch,
                    keymap::Command::SettingsPrevious,
                    keymap::Command::SettingsNext,
                ]);
            }
            Some(allowed)
        } else {
            None
        };
        keymap::restrict(ctx, allowed);
        let commands = if capturing {
            keymap::cancel(ctx, keymap::Scope::Window);
            Vec::new()
        } else if modal_open {
            keymap::pressed(ctx, keymap::Scope::Window)
                .into_iter()
                .filter(|c| matches!(c, keymap::Command::Manual | keymap::Command::Palette))
                .collect()
        } else {
            keymap::pressed(ctx, keymap::Scope::Window)
        };
        let mut refresh = false;
        for command in commands {
            refresh |= self.window_command(command, modal_open, ctx);
        }
        // Coming back to the window (e.g. after using git elsewhere) refreshes the open repo.
        let regained = focused && !self.was_focused;
        self.was_focused = focused;
        if let Some(Tab::Repo(r)) = self.tabs.get_mut(self.active)
            && (refresh || regained)
        {
            r.refresh(ctx);
        }
        focused
    }

    /// Does what the key of `command` does; `modal_open`: a dialog, the palette, the manual or a
    /// text field has the keyboard. Returns whether the open repository is to be read again.
    fn window_command(
        &mut self,
        command: keymap::Command,
        modal_open: bool,
        ctx: &egui::Context,
    ) -> bool {
        use keymap::Command;
        // Not over a dialog that asks something (the clone, the remote one, ssh's question).
        let asking = self.window_has_dialog() || self.askpass_open();
        match command {
            Command::Manual if !asking && self.palette.is_none() => {
                self.manual_open = !self.manual_open;
            }
            Command::Palette if !asking => {
                if self.palette.is_some() {
                    self.palette = None;
                } else {
                    self.open_palette();
                }
            }
            Command::Manual | Command::Palette => {}
            _ if modal_open => {}
            Command::Settings => self.open_settings(),
            Command::NewTab => self.new_tab(),
            Command::ReopenTab => self.reopen_closed_tab(ctx),
            Command::CloseTab => self.close_tab(self.active, ctx),
            Command::NextTab => self.step_tab(1),
            Command::PreviousTab => self.step_tab(-1),
            Command::GoToTab(place) => self.go_to_tab(usize::from(place)),
            Command::LastTab => self.go_to_last_tab(),
            Command::Reload => return true,
            // Contextual actions are delivered by their owning view.
            _ => {}
        }
        false
    }

    /// The active tab's page under the strip.
    fn active_tab_ui(&mut self, ui: &mut Ui, t: &Tokens) {
        let ctx = ui.ctx().clone();
        let active = self.active;
        let asking = self.askpass_open();
        match self.tabs.get_mut(active) {
            Some(Tab::Repo(repo)) => {
                repo.keys_blocked = self.palette.is_some()
                    || self.manual_open
                    || self.clone_form.is_some()
                    || self.remote_form.is_some()
                    || self.teleport.is_some()
                    || self.repo_scan.is_some()
                    || self.my_work.open
                    || asking;
                // Its widgets' state (scroll offsets, text fields' undo, open menus) is its own:
                // every repository tab lays out the same widgets, which would otherwise share
                // it with the tab shown before.
                let scope = repo_scope(&repo.path);
                ui.push_id(scope, |ui| repo.ui(ui));
                // A tab asked for a folder to be opened.
                match repo.open_request.take() {
                    Some(OpenRequest::NewTab(path)) => self.open_repo(&path, &ctx),
                    Some(OpenRequest::Worktree(path)) => self.open_worktree(&path, &ctx),
                    None => {}
                }
            }
            Some(Tab::Settings) => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(t.dock))
                    .show(ui, |ui| self.settings_page(ui));
            }
            _ => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(t.dock))
                    .show(ui, |ui| self.home(ui));
            }
        }
    }
}

impl App {
    /// Model work only: called by eframe even while the root has no painting pass, and
    /// by deferred children. Channel draining and service deadlines keep a single owner.
    fn tick_model(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        // eframe also accepts Close from logic-only passes of hidden/minimized roots.
        self.close_root_with_children(ctx);
        self.wake_askpass_owner(ctx);
        self.settings = Settings::get(ctx);
        crate::portable::set_credential_store(self.settings.portable_credential_store);
        crate::network::set_choices(self.settings.offline, self.settings.pause_on_metered);
        crate::network::start_watching(ctx);
        self.take_handoffs(ctx);
        if self.git_version.is_err() {
            return;
        }
        self.poll_clone(ctx);
        self.poll_connecting(ctx);
        self.poll_launch(ctx);
        self.poll_window_models(ctx);
        self.poll_update(ctx);
        self.poll_inbox_opening(ctx);
        self.agent_requests(ctx);
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.windows.parked.values_mut().flat_map(|w| &mut w.tabs))
        {
            if let Tab::Repo(r) = tab {
                r.poll(ctx);
                r.background_tick();
                r.poll_parked(ctx);
            }
        }
        self.advance_patches(ctx);
        if let Some(frame) = frame {
            self.os_shell(ctx, frame);
        }
        let focused = ctx.input(|i| i.raw.viewports.values().any(|v| v.focused == Some(true)));
        self.keep_drafts(ctx, focused);
        self.toast_finished(focused);
        // A dormant root still services completions/launches, even when every child is
        // minimized. This schedules logic; it does not force a hidden window to paint.
        if self.windows.enabled && !self.windows.parked.is_empty() {
            ctx.request_repaint_after_for(
                std::time::Duration::from_millis(250),
                egui::ViewportId::ROOT,
            );
        }
    }

    fn window_ui(&mut self, ui: &mut Ui, frame: Option<&mut eframe::Frame>) {
        let ctx = ui.ctx().clone();
        if frame.is_none() {
            self.tick_model(&ctx, None);
        }
        if std::mem::take(&mut self.pick_on_start) {
            self.pick_existing(&ctx);
        }
        crate::image_cache::maintain(&ctx);
        let t = Tokens::get(&ctx);
        // A button dropped moves in the settings, read just below.
        crate::app_buttons::begin_frame(&ctx);
        // Column widths dragged in a graph header live in the context; keep them for saving.
        self.settings = Settings::get(&ctx);
        // A theme file being edited, VS Code's theme switched: shown as they change.
        theme::poll(&ctx);
        crate::portable::set_credential_store(self.settings.portable_credential_store);
        crate::network::set_choices(self.settings.offline, self.settings.pause_on_metered);
        crate::network::start_watching(&ctx);
        title_bar::apply(&ctx, &self.settings, &mut self.decorated);
        if let Some(frame) = frame {
            title_bar::sync_border(frame, &self.settings, &mut self.window_border);
        }
        self.close_root_with_children(&ctx);
        crate::app_buttons::set_current_theme(&ctx, &self.theme);

        // eframe shows the window after the first frame; maximize it on the next one.
        if MAXIMIZE_WHEN_SHOWN.load(Ordering::Relaxed) {
            if ctx.cumulative_frame_nr() >= 1 {
                MAXIMIZE_WHEN_SHOWN.store(false, Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            } else {
                ctx.request_repaint();
            }
        }

        if let Err(err) = self.git_version.clone() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t.dock))
                .show(ui, |ui| self.git_missing(ui, &err));
            return;
        }
        self.notice_profile_git(&ctx);
        let focused = self.shortcuts(&ctx);

        // The caption buttons go up to the window's corner.
        let strip_right = if title_bar::wanted(&self.settings) {
            0
        } else {
            6
        };
        egui::Panel::top("tabs")
            .frame(
                egui::Frame::new()
                    .fill(t.tab_strip)
                    .inner_margin(egui::Margin {
                        // The first tab starts at the window's edge: a gap before it shows as a
                        // dark band beside it when it is the active one.
                        left: 0,
                        right: strip_right,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| self.tab_strip(ui));
        self.update_banner(ui);
        if let Some(path) = self.switch_to.take() {
            self.open_worktree(&path, &ctx);
        }
        if std::mem::take(&mut self.close_worktree) {
            self.close_current_worktree(&ctx);
        }

        self.active_tab_ui(ui, &t);
        // Views popped out of any tab keep their windows, the tab shown or not; after it, so
        // they show what it just took in.
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if let Tab::Repo(r) = tab {
                r.popouts_ui(&ctx, i == self.active);
            }
        }
        self.clone_dialog(&ctx);
        self.remote_dialog(&ctx);
        self.teleport_dialog(&ctx);
        self.askpass_dialog(&ctx);
        self.agent_card(&ctx);
        self.palette_ui(&ctx);
        self.manual_ui(&ctx);
        // After the tabs are drawn, so what was typed in this frame is in.
        self.keep_drafts(&ctx, focused);
        self.repo_scan_dialog(&ctx);
        self.my_work_ui(&ctx);
        self.poll_diagnostics(&ctx);
        self.toast_finished(focused);
        self.toasts_ui(&ctx);
        self.dropped_files(&ctx);
        self.app_button_events(&ctx);
        crate::app_buttons::end_frame(&ctx);
        if title_bar::wanted(&self.settings) {
            title_bar::resize_edges(&ctx);
        }
        keymap::which_key(&ctx, |command| match self.tabs.get(self.active) {
            Some(Tab::Repo(r)) => r.command_refusal(command),
            _ if command.scope() == keymap::Scope::Repository => {
                Some("Open a repository first".into())
            }
            _ => None,
        });
        crate::a11y::end_frame(&ctx);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.tick_model(ctx, Some(frame));
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        self.window_ui(ui, Some(frame));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // Index of the active tab among the saved (repo) tabs.
        let (mut open, active) = self.open_repos();
        let detached = self.saved_windows();
        open.retain(|p| !detached.iter().any(|w| w.open.contains(p)));
        let saved = Saved {
            open,
            active,
            recent: self.recent.clone(),
            theme: self.theme.clone(),
            clone_parent: self.clone_parent.clone(),
            settings: self.settings.clone(),
            skipped_version: self.skipped_version.clone(),
            pinned_version: self.pinned_version.clone(),
            tab_looks: self.tab_looks.clone(),
            profile_states: self.profile_states.clone(),
            palette_recent: self.palette_recent.clone(),
            last_opened: self.last_opened.clone(),
            workspaces: self.workspaces.clone(),
            found_repos: self.found_repos.clone(),
            inbox_seen: self.inbox_seen.clone(),
            windows: detached,
        };
        eframe::set_value(storage, eframe::APP_KEY, &saved);
    }

    fn on_exit(&mut self) {
        self.stop_teleport();
        let owners = self.windows.parked.keys().copied().collect::<Vec<_>>();
        for owner in owners {
            self.in_window(owner, Self::stop_teleport);
        }
        self.close_drafts();
        crate::instance::closing();
    }
}

/// Where the tab that was active at `active` is once the tabs not `kept` (their folder is gone)
/// are left out: a gone one passes to the tab before it.
fn active_among_kept(kept: &[bool], active: usize) -> usize {
    kept.iter()
        .take(active.saturating_add(1))
        .filter(|&&k| k)
        .count()
        .saturating_sub(1)
}

/// What the widgets of the repository tab of `path` are told apart from another tab's by.
fn repo_scope(path: &Path) -> egui::Id {
    egui::Id::new(("repo-tab", path))
}

/// The top of the repository `path` is in, in the form tabs are told apart by.
pub(crate) fn repo_root(path: &Path) -> Result<PathBuf, String> {
    crate::launch::resolve::repo_root(path)
}

pub use draft_saving::keep_no_drafts;

/// When this process was started as `gitcraft checkpoint` by a coding agent's hook: saves the
/// turn and returns the exit code to end with, before any window is made. `None` otherwise.
pub fn checkpoint_cli() -> Option<i32> {
    crate::agent::checkpoint::cli()
}

/// When this process was started by ssh as its askpass program: asks the running app ssh's
/// question and returns the exit code to end with, before any window is made. `None` for an
/// ordinary start.
pub fn askpass_helper() -> Option<i32> {
    crate::remote::askpass::run_helper()
}

/// How many commands for a repository on another machine were asked from the window's thread
/// (and refused: it never waits for a network). For tests and `examples/ui_shot.rs`.
pub fn refused_on_ui_thread() -> usize {
    crate::remote::refused_on_ui_thread()
}

/// Work the first frame will need, started before the window is made so that it overlaps with
/// that: on macOS, asking the login shell for its PATH, which every git call waits for (its
/// profile scripts can take seconds).
pub fn warm_up() {
    std::thread::spawn(|| {
        let _ = crate::shell_path::path();
    });
}

/// The file the window's state is saved in, when it isn't eframe's own default place: in the
/// portable folder when gitcraft runs portable.
pub fn persistence_path() -> Option<PathBuf> {
    crate::portable::persistence_path()
}

pub use crate::instance::Start;

/// Settles, before any window is made, whether this start opens the window or hands what it was
/// started for (a folder, or a command line such as `log <file>`) to the gitcraft already running
/// (see `crate::instance`).
pub fn single_instance(
    launch: Option<&crate::launch::Command>,
    args: &[std::ffi::OsString],
) -> Start {
    use crate::launch::Command;
    match launch {
        None => crate::instance::start(None),
        Some(Command::Open(path)) => crate::instance::start(Some(path)),
        // `log <file>`…: the command line as typed, read again by the running window.
        Some(_) => crate::instance::start_launch(args),
    }
}

/// Tells the user why this start ends without a window: in a dialog, and on stderr for a start
/// from a terminal.
/// What the command line asked to be told (`--help`, a mistake in it): on the terminal it was
/// typed in, or in a dialog when there is none to see it (a Windows release has no console; a
/// menu or a file manager starts gitcraft without one).
pub fn tell(message: &str, error: bool) {
    use std::io::IsTerminal;
    let terminal = !cfg!(all(windows, not(debug_assertions))) && std::io::stderr().is_terminal();
    match (terminal, error) {
        (true, true) => eprintln!("gitcraft: {message}"),
        (true, false) => println!("{message}"),
        (false, true) => report_failed_start(message),
        (false, false) => {
            let _ = rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Info)
                .set_title("gitcraft")
                .set_description(message)
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        }
    }
}

pub fn report_failed_start(message: &str) {
    eprintln!("gitcraft: {message}");
    // rfd's dialogs are zenity's on Linux; without it (KDE, minimal installs) a start from the
    // menu would end with nothing shown: a desktop notification says it instead.
    if cfg!(all(unix, not(target_os = "macos"))) && !on_path("zenity") {
        let _ = notify_rust::Notification::new()
            .appname("gitcraft")
            .summary("gitcraft")
            .body(&panels::notification_body(message))
            .show();
        return;
    }
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("gitcraft")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Whether a program called `name` is on the PATH.
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// Once the window closed: lets the next start be the running instance, then starts the
/// version an update put in place, if it did.
pub fn after_exit() {
    crate::tasks::shutdown();
    crate::parallel_bisect::shutdown();
    crate::processes::shutdown();
    crate::remote::shutdown();
    crate::instance::shut_down();
    update::relaunch();
}

/// The window's native frame, configured before creation from the saved title-bar setting.
pub fn window_viewport() -> egui::ViewportBuilder {
    let app_ron = eframe::storage_dir("gitcraft").map(|dir| dir.join("app.ron"));
    title_bar::viewport(title_bar::decorated_at_start(app_ron.as_deref()))
}

/// The window and taskbar icon (the `.exe` gets the same picture from `build.rs`).
pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/app-icon/gitcraft-256.png"))
        .unwrap_or_default()
}

/// `canonicalize` on Windows returns `\\?\C:\…`; show and store the familiar form.
pub(crate) fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
}
