//! User settings: stored with the app state, readable from anywhere through the egui context.

mod agents;
mod ai_page;
mod appearance_page;
mod branch_names;
mod connection_page;
mod context_menus;
mod custom_commands;
mod diffs;
mod external_tools;
mod file_menus;
mod general_page;
mod git_page;
mod graph_page;
mod identities_page;
mod labels;
mod language;
mod layout;
pub(crate) mod network_page;
mod page;
mod page_kind;
mod portable_page;
mod preview;
mod profile_page;
pub mod profiles;
mod render;
mod signing;
mod terminal_page;
mod themes;
mod tips_row;
mod view_preferences;
mod worktree_archives;

use egui::Ui;
use serde::{Deserialize, Serialize};

pub(crate) use branch_names::fit as fit_branch_name;
pub use context_menus::ContextMenus;
pub use graph_page::HistoryMap;
#[cfg(test)]
use layout::Filter;
pub(crate) use page::show as show_page;
pub(crate) use page_kind::Page;
pub use profile_page::{ProfileRequest, SettingsView, paint_badge};
pub use profiles::UserProfile;
pub use themes::{ThemePick, theme_menu};
pub use worktree_archives::WorktreeArchives;

/// A name and email to commit with, applied per repository from its commit box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedIdentity {
    pub label: String,
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Empty follows OS language preferences; unknown stored tags fall back to English.
    pub language: String,
    /// Interface zoom (1.0 = 100 %).
    pub ui_scale: f32,
    /// Stable action IDs mapped to zero (unbound) or up to four key strokes.
    pub(crate) key_bindings: crate::keymap::Overrides,
    pub terminal_font: f32,
    /// Program of the default shell; empty = the platform's default.
    pub shell: String,
    /// Width of the branch/tag column of the graph.
    pub refs_width: f32,
    pub history_filter: crate::repo::history_filter::Rules,
    /// Fit Branch / Tag to visible pills; dragging its border switches to a fixed width.
    pub auto_refs_width: bool,
    /// Shortened branch and tag pills keep their end rather than their beginning.
    pub branch_names_keep_end: bool,
    /// Width of the graph column; 0 = fit the lanes of the repository.
    pub graph_width: f32,
    /// The graph's lanes closer together, for a narrower graph column.
    pub compact_graph: bool,
    pub graph_motion: bool,
    pub graph_lane_walk: bool,
    pub graph_sounds: bool,
    pub celebration_motion: bool,
    pub celebration_sound: bool,
    /// Widths of the author, when, date and hash columns (dragged in the graph header).
    pub author_width: f32,
    pub when_width: f32,
    pub date_width: f32,
    pub hash_width: f32,
    /// Legacy drag metadata, retained for saved-settings compatibility; sizing ignores it.
    pub last_resized: Option<crate::repo::columns::Bar>,
    /// Legacy cascading-resize offset; new divider drags clear it.
    pub column_push: f32,
    /// Description space and sibling compression retained by the most recent column gesture.
    pub description_space: crate::repo::columns::DescriptionSpace,
    pub show_author: bool,
    /// How long ago each commit was made ("3 hours ago").
    pub show_when: bool,
    /// An absolute date column.
    pub show_date: bool,
    pub show_hash: bool,
    /// The test lane: a pass / fail dot by each commit for the repository's test command.
    pub show_test_lane: bool,
    /// The branches panel on the left of a repository tab.
    pub show_branches: bool,
    /// The commit detail / changes panel on the right of a repository tab.
    pub show_detail: bool,
    /// A worktree opens in the tab it was opened from, which then shows its branch.
    pub worktrees_in_same_tab: bool,
    /// Windows and macOS: the tab strip stands in for the system's title bar.
    pub tabs_in_title_bar: bool,
    /// How large the history map beside the graph is, or whether it shows at all.
    pub history_map: HistoryMap,
    /// Where the settings, command palette, theme and manual buttons sit, and in what order.
    pub button_layout: crate::app_buttons::ButtonLayout,
    /// Changed files in a tree of their folders rather than a flat list.
    pub files_as_tree: bool,
    /// Diffs side by side rather than unified.
    pub diff_split: bool,
    pub diff_wrap: bool,
    /// Diffs show the whole file, not just the lines around the changes.
    pub diff_whole_file: bool,
    /// Diffs of commits and of the working tree show below the graph, not in its place.
    pub diff_beside_graph: bool,
    /// The diff algorithm chosen for each file type (see [`crate::git::file_type`]); a type
    /// not listed uses git's own default.
    pub diff_algorithms: std::collections::BTreeMap<String, crate::git::DiffAlgorithm>,
    /// Generated files, lockfiles and minified files start folded in file lists and diffs.
    pub fold_generated: bool,
    /// More files to fold: glob patterns separated by commas, spaces or new lines.
    pub fold_patterns: String,
    /// Background fetch interval in minutes; 0 turns it off.
    pub auto_fetch_minutes: u32,
    /// Offline mode: nothing goes on the network by itself, and fetch, pull, push and the like
    /// are refused with why (see `network`).
    pub offline: bool,
    /// On a metered connection (as the system reports it), the background work pauses.
    pub pause_on_metered: bool,
    /// What the toolbar's Pull does.
    pub pull_mode: crate::repo::pull::PullMode,
    /// Load avatars: GitHub / Forgejo / Gitea / GitLab accounts on repositories hosted there,
    /// Gravatar otherwise.
    pub avatars: bool,
    /// Command that opens a folder or file in the user's editor.
    pub editor: String,
    /// Desktop notifications when a long operation ends while the window is in the background.
    pub notifications: bool,
    /// Opt-in resident coordinator; native objects and hidden state are never persisted.
    pub keep_running_in_tray: bool,
    /// Ask GitHub on every start whether a newer release exists.
    pub check_updates: bool,
    /// Which releases: the main ones, or also a build of every change to dev.
    pub update_channel: crate::update::Channel,
    /// Download a newer release in the background, ready to go in place with a restart.
    pub download_updates: bool,
    /// Colour code in diffs and blame with VS Code's current theme, when it has one.
    pub vscode_colors: bool,
    /// What is changed about whichever theme is in use: accent, corners, lane colours.
    pub theme_tweaks: crate::theme::Tweaks,
    /// Offer Mergiraf in the conflict editor, when it is installed.
    pub mergiraf: bool,
    /// The diff tool _Open in diff tool_ starts (git's `diff.tool` unless a preset is chosen).
    pub diff_tool: crate::git::external_tool::ToolChoice,
    /// The merge tool _Resolve in merge tool_ starts (git's `merge.tool` unless a preset is
    /// chosen).
    pub merge_tool: crate::git::external_tool::ToolChoice,
    /// After a checkout, pull, merge or rebase, update the submodules to the commits it records.
    pub keep_submodules: bool,
    /// Running portable: keep sign-ins in this computer's credential store all the same.
    pub portable_credential_store: bool,
    /// A local branch with no commit for this many days is marked stale; 0 marks none.
    pub stale_branch_days: u32,
    /// Where cleaned-up worktrees' untracked files are archived, and how much they may take.
    pub worktree_archives: WorktreeArchives,
    /// Saved as `profiles` by the versions before profiles were what they are now.
    #[serde(rename = "profiles")]
    pub identities: Vec<SavedIdentity>,
    /// The profiles to switch between; there is always at least one.
    pub user_profiles: Vec<UserProfile>,
    /// The id of the profile in use.
    pub active_profile: u64,
    /// The AI tool that writes commit messages and reviews branches (Settings → AI).
    pub ai: crate::ai::AiSettings,
    /// Commands added to the menus and the palette (Settings → Custom commands).
    pub custom_commands: Vec<crate::custom_commands::CustomCommand>,
    /// The repositories' command files the user trusted, as they were then.
    pub trusted_command_files: Vec<crate::custom_commands::TrustedFile>,
    /// Agents connected over `gitcraft mcp` may change repositories (through the window, as
    /// steps of the undo list); otherwise they only read.
    pub agent_writes: bool,
    /// Show the optional task section; independent of MCP write permission.
    pub show_agents: bool,
    /// Optional commands in branch and commit context menus.
    pub context_menus: ContextMenus,
    /// The "Did you know" line and the first-time hints of a repository tab.
    pub tips: bool,
    /// The first-time hints shown already (by their ids), each shown only once.
    pub seen_hints: std::collections::BTreeSet<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            language: String::new(),
            key_bindings: Default::default(),
            terminal_font: 12.5,
            shell: String::new(),
            refs_width: DEFAULT_REFS_WIDTH,
            history_filter: Default::default(),
            auto_refs_width: true,
            branch_names_keep_end: true,
            graph_width: 0.0,
            compact_graph: false,
            graph_motion: true,
            graph_lane_walk: false,
            graph_sounds: false,
            celebration_motion: false,
            celebration_sound: false,
            author_width: 150.0,
            when_width: 100.0,
            date_width: 118.0,
            hash_width: 70.0,
            last_resized: None,
            column_push: 0.0,
            description_space: Default::default(),
            show_author: true,
            show_when: true,
            show_date: false,
            show_hash: false,
            show_test_lane: false,
            show_branches: true,
            show_detail: true,
            worktrees_in_same_tab: true,
            tabs_in_title_bar: true,
            history_map: Default::default(),
            button_layout: Default::default(),
            files_as_tree: true,
            diff_split: false,
            diff_wrap: false,
            diff_whole_file: false,
            diff_beside_graph: false,
            diff_algorithms: Default::default(),
            fold_generated: true,
            fold_patterns: String::new(),
            auto_fetch_minutes: 10,
            offline: false,
            pause_on_metered: true,
            pull_mode: Default::default(),
            avatars: true,
            editor: "code".to_owned(),
            notifications: true,
            keep_running_in_tray: false,
            check_updates: true,
            update_channel: Default::default(),
            download_updates: true,
            vscode_colors: true,
            theme_tweaks: Default::default(),
            mergiraf: true,
            diff_tool: Default::default(),
            merge_tool: Default::default(),
            keep_submodules: false,
            portable_credential_store: false,
            stale_branch_days: 90,
            worktree_archives: Default::default(),
            identities: Vec::new(),
            user_profiles: vec![UserProfile::default()],
            active_profile: UserProfile::default().id,
            ai: Default::default(),
            custom_commands: Vec::new(),
            trusted_command_files: Vec::new(),
            agent_writes: true,
            show_agents: false,
            context_menus: Default::default(),
            tips: true,
            seen_hints: Default::default(),
        }
    }
}

fn id() -> egui::Id {
    egui::Id::new("gitcraft-settings")
}

/// What crate::i18n::text("interaction-settings-compact") does.
pub(crate) const DEFAULT_REFS_WIDTH: f32 = 280.0;

/// Localized prose for the compact graph setting.
pub fn compact_tip() -> &'static str {
    crate::i18n::text("interaction-settings-compact-tip")
}

impl Settings {
    pub fn get(ctx: &egui::Context) -> Settings {
        ctx.data(|d| d.get_temp::<Settings>(id()))
            .unwrap_or_default()
    }

    /// Older settings with a custom ref width keep that explicit preference.
    pub(crate) fn fit_visible_refs(&self) -> bool {
        self.auto_refs_width && self.refs_width == DEFAULT_REFS_WIDTH
    }

    /// Read just the history map's size without cloning the settings every frame.
    pub(crate) fn history_map(ctx: &egui::Context) -> HistoryMap {
        ctx.data_mut(|d| d.get_temp_mut_or_default::<Settings>(id()).history_map)
    }

    /// Read just the pill shortening preference without cloning the settings for every pill.
    pub(crate) fn branch_names_keep_end(ctx: &egui::Context) -> bool {
        ctx.data_mut(|d| {
            d.get_temp_mut_or_default::<Settings>(id())
                .branch_names_keep_end
        })
    }

    /// Gives the graph's columns their first widths back; which of them show stays.
    pub fn reset_columns(&mut self) {
        let fresh = Settings::default();
        self.refs_width = fresh.refs_width;
        self.auto_refs_width = fresh.auto_refs_width;
        self.graph_width = fresh.graph_width;
        self.author_width = fresh.author_width;
        self.when_width = fresh.when_width;
        self.date_width = fresh.date_width;
        self.hash_width = fresh.hash_width;
        self.last_resized = fresh.last_resized;
        self.column_push = fresh.column_push;
        self.description_space = fresh.description_space;
    }

    /// Puts the graph's lanes closer together or back apart. The graph column then fits them
    /// again, or it would stay as wide.
    pub fn set_compact_graph(&mut self, compact: bool) {
        self.compact_graph = compact;
        self.graph_width = 0.0;
    }

    /// Puts these settings in place for a test, without what `set` does around them (the
    /// zoom, the profile's git config, which are the whole process's).
    #[cfg(test)]
    pub(crate) fn put_for_test(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(id(), self.clone()));
    }

    pub fn set(&self, ctx: &egui::Context) {
        if crate::i18n::set_language(&self.language) {
            ctx.request_repaint();
        }
        ctx.data_mut(|d| d.insert_temp(id(), self.clone()));
        ctx.set_zoom_factor(self.ui_scale.clamp(0.7, 2.0));
        crate::vscode_theme::set_enabled(self.vscode_colors);
        profiles::apply(&self.profile());
    }

    /// There is a profile, and the active one is among them.
    pub fn ensure_profile(&mut self) {
        if self.user_profiles.is_empty() {
            self.user_profiles.push(UserProfile::default());
        }
        if !self
            .user_profiles
            .iter()
            .any(|p| p.id == self.active_profile)
        {
            self.active_profile = self.user_profiles.first().map_or(1, |p| p.id);
        }
    }

    /// The profile in use.
    pub fn profile(&self) -> UserProfile {
        self.user_profiles
            .iter()
            .find(|p| p.id == self.active_profile)
            .or(self.user_profiles.first())
            .cloned()
            .unwrap_or_default()
    }

    /// The profile in use, to change.
    pub fn profile_mut(&mut self) -> &mut UserProfile {
        self.ensure_profile();
        let active = self.active_profile;
        let i = self
            .user_profiles
            .iter()
            .position(|p| p.id == active)
            .unwrap_or(0);
        &mut self.user_profiles[i]
    }

    /// Adds a profile called `name`, a copy of the one in use when `copy`, and returns its id.
    /// It is not switched to: that is the app's, which swaps the tabs.
    pub fn add_profile(&mut self, name: &str, copy: bool) -> u64 {
        let id = self.user_profiles.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut profile = if copy {
            self.profile()
        } else {
            UserProfile::default()
        };
        profile.id = id;
        profile.name = name.to_owned();
        self.user_profiles.push(profile);
        id
    }

    /// `base`, or `base 2`, `base 3`… whichever no profile is called yet.
    pub fn new_profile_name(&self, base: &str) -> String {
        let taken = |name: &str| self.user_profiles.iter().any(|p| p.name == name);
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }
}

/// Parse the editable numbers we format for the selected locale. Without the matching
/// parser, egui would read German "2.000" as two rather than two thousand.
fn parse_localized_number(input: &str) -> Option<f64> {
    let (group, decimal) = match crate::i18n::language() {
        crate::i18n::Language::English => (',', '.'),
        crate::i18n::Language::German => ('.', ','),
    };
    let input = input.trim();
    let mut parts = input.split(decimal);
    let integer = parts.next()?;
    let fraction = parts.next();
    if parts.next().is_some() || fraction.is_some_and(|s| s.contains(group)) {
        return None;
    }
    let unsigned = integer.strip_prefix(['-', '+']).unwrap_or(integer);
    let groups: Vec<_> = unsigned.split(group).collect();
    if groups.len() > 1
        && (groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|s| s.len() != 3))
    {
        return None;
    }
    let value = input
        .replace(group, "")
        .replace(decimal, ".")
        .parse::<f64>()
        .ok()?;
    value.is_finite().then_some(value)
}

/// Where the look for a newer version stands, as the settings page shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateCheck {
    /// Not looked yet (looking on start is off).
    #[default]
    NotYet,
    Checking,
    UpToDate,
    /// This version is out.
    Found(String),
    Failed(String),
    /// Gone back to this version: no update is offered until it is unpinned.
    Pinned(String),
}

/// Width of the drop-down lists, so they line up.
const COMBO_WIDTH: f32 = 230.0;
/// Width of the text fields.
const FIELD_WIDTH: f32 = 300.0;
/// Width of the sliders' track.
const SLIDER_WIDTH: f32 = 240.0;

/// A note under a checkbox, lined up with the checkbox's text.
fn indented_note(ui: &mut Ui, text: impl Into<String>) {
    let s = ui.spacing();
    let indent = s.icon_width + s.icon_spacing;
    // Close under the checkbox, whose row is taller than its text.
    ui.add_space(-6.0);
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: indent as i8,
            ..Default::default()
        })
        .show(ui, |ui| layout::note(ui, text));
}

#[cfg(test)]
mod tests;
