//! Window shell: git check, browser-like tabs, the start page and cloning.

mod commands;
mod construction;
mod frame;
mod lifecycle;
mod repositories;
mod updates;
pub use lifecycle::*;

mod agent_card;
mod askpass_dialog;
mod clone;
mod diagnostics;
mod draft_saving;
mod dropped;
mod focus;
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
mod practice;
mod profiles;
mod recent_status;
mod remote_open;
mod resident;
pub(crate) use resident::notification_body as resident_notification_body;
mod repo_scan;
mod setup;
mod startup_display;
mod startup_language;
pub use startup_language::initialize_language;
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
use home::AppError;
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
    resident: crate::tray::Resident,
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
    error: Option<AppError>,
    /// What the start page's error is, when it is a git failure gitcraft can explain (see
    /// [`crate::error_card`]).
    error_card: Option<crate::error_card::ErrorCard>,
    focus: focus::State,
    clone_form: Option<CloneForm>,
    practice_open: bool,
    setup: setup::SetupWindow,
    practice_jobs: crate::tasks::Scope,
    practicing: Option<practice::Practicing>,
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
    menu_for: Option<(u64, Vec<PathBuf>)>,
    /// The dialog that opens a repository in WSL or over SSH.
    remote_form: Option<RemoteForm>,
    /// Repositories on other machines being reached, each tab to open once it answers.
    connecting: Vec<Connecting>,
    /// The teleport dialog, and the teleport it runs.
    teleport: Option<teleport::TeleportForm>,
    teleport_drag: Option<teleport::DragTargets>,
    /// ssh's question being answered (a password, a new host key).
    askpass: askpass_dialog::AskpassState,
    /// A newer release than this build.
    update: Option<update::Found>,
    update_discovery: updates::Discovery,
    /// The update banner was put off until the next start.
    update_later: bool,
    /// Why the update didn't go in place.
    update_error: Option<String>,
    /// Owned source receipt for exactly the raw installation failure above.
    update_failure: Option<update::Failure>,
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
    fn set_theme(&mut self, ctx: &egui::Context, id: ThemeId) {
        self.theme = id;
        self.apply_theme(ctx);
    }

    /// Paints the app with its theme and the settings' tweaks of it.
    fn apply_theme(&self, ctx: &egui::Context) {
        theme::apply(ctx, &self.theme, &self.settings.theme_tweaks);
    }
}
