//! User settings: stored with the app state, readable from anywhere through the egui context.

mod agents;
mod ai_page;
mod branch_names;
mod context_menus;
mod custom_commands;
mod diffs;
mod external_tools;
mod file_menus;
mod layout;
mod page;
mod portable_page;
mod preview;
mod profile_page;
pub mod profiles;
mod signing;
mod themes;
mod tips_row;

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::theme::Tokens;
pub(crate) use branch_names::fit as fit_branch_name;
pub use context_menus::ContextMenus;
use file_menus::file_menu_rows;
use layout::{Card, Filter, section};
pub(crate) use page::show as show_page;
use preview::Preview;
pub use profile_page::{ProfileRequest, SettingsView, paint_badge};
pub use profiles::UserProfile;
pub use themes::{ThemePick, theme_menu};

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
    /// Interface zoom (1.0 = 100 %).
    pub ui_scale: f32,
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
    /// Where the settings, command palette, theme and manual buttons sit, and in what order.
    pub button_layout: crate::app_buttons::ButtonLayout,
    /// Changed files in a tree of their folders rather than a flat list.
    pub files_as_tree: bool,
    /// Diffs side by side rather than unified.
    pub diff_split: bool,
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

/// What offline mode does.
const OFFLINE_TIP: &str = "Nothing goes on the network by itself (background fetch, avatars, \
update checks, build and pull request polling), and Fetch, Pull, Push and the like say why \
they don't run instead of waiting for a network that isn't there.";

/// What pausing on a metered connection does.
const METERED_TIP: &str = "When the system says the connection is metered (Windows: a metered \
or roaming network; Linux: NetworkManager), the background fetch, avatars, update checks and \
downloads and the polling of builds and pull requests wait (a repository over SSH still \
fetches: on that machine's network). What you ask for still runs.";

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            terminal_font: 12.5,
            shell: String::new(),
            refs_width: DEFAULT_REFS_WIDTH,
            history_filter: Default::default(),
            auto_refs_width: true,
            branch_names_keep_end: true,
            graph_width: 0.0,
            compact_graph: false,
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
            tabs_in_title_bar: false,
            button_layout: Default::default(),
            files_as_tree: true,
            diff_split: false,
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

/// What "Compact graph" does.
pub(crate) const DEFAULT_REFS_WIDTH: f32 = 280.0;

pub const COMPACT_TIP: &str = "Lanes closer together, for a narrower graph column";

impl Settings {
    pub fn get(ctx: &egui::Context) -> Settings {
        ctx.data(|d| d.get_temp::<Settings>(id()))
            .unwrap_or_default()
    }

    /// Older settings with a custom ref width keep that explicit preference.
    pub(crate) fn fit_visible_refs(&self) -> bool {
        self.auto_refs_width && self.refs_width == DEFAULT_REFS_WIDTH
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

    /// Mergiraf: whether it is installed (and a button to install it), whether to offer it in
    /// the conflict editor, and whether git merges with it.
    fn mergiraf_ui(&mut self, card: &mut Card) {
        use crate::mergiraf::{self, Job};
        let ctx = card.ctx();
        let t = Tokens::get(&ctx);
        let state = mergiraf::state(&ctx);
        let job = mergiraf::job();
        let busy = matches!(job, Job::Running(_));
        card.row("Installed", "install version download codeberg", |ui| {
            ui.horizontal(|ui| {
                match &state.found {
                    Some(found) => {
                        let what = if found.own {
                            format!("{} (downloaded by gitcraft)", found.version)
                        } else {
                            found.version.clone()
                        };
                        ui.label(RichText::new(what).color(t.text_dim));
                    }
                    None => {
                        if ui
                            .add_enabled(!busy, egui::Button::new("Install"))
                            .on_hover_text(
                                "Download the latest Mergiraf for this system from its releases \
                                 on codeberg.org into gitcraft's folder. It stays Mergiraf's \
                                 (GPL-3.0); gitcraft only runs it.",
                            )
                            .clicked()
                        {
                            mergiraf::install(ui.ctx());
                        }
                        ui.hyperlink_to("Other ways", mergiraf::DOCS);
                        if matches!(job, Job::Idle) {
                            layout::note(ui, "Not installed");
                        }
                    }
                }
                match &job {
                    Job::Idle => {}
                    Job::Running(doing) => {
                        ui.add(crate::spinner::Spinner::new());
                        layout::note(ui, *doing);
                    }
                    Job::Done(done) => {
                        ui.label(RichText::new(done).color(t.success).small());
                    }
                    Job::Failed(e) => {
                        ui.label(RichText::new(e).color(t.danger).small());
                    }
                }
            });
        });
        card.row("Conflict editor", "offer answer resolve", |ui| {
            ui.add_enabled(
                state.found.is_some(),
                egui::Checkbox::new(&mut self.mergiraf, "Offer Mergiraf's answer"),
            )
            .on_hover_text(
                "Mergiraf solves conflicts from the syntax of the code, in many languages. \
                 Its answer shows in the editor; nothing is saved before you check it.",
            );
        });
        card.row(
            "Git merges",
            "every merge rebase cherry-pick driver",
            |ui| {
                let mut in_merges = state.in_merges;
                let toggle = ui
                    .add_enabled(
                        state.found.is_some() && !busy,
                        egui::Checkbox::new(&mut in_merges, "Use it in every git merge"),
                    )
                    .on_hover_text(
                        "Git merges, rebases and cherry-picks go through Mergiraf, in every \
                     repository on this machine and in the terminal too, so fewer conflicts \
                     are left at all (not in WSL or over SSH, where git is that machine's). \
                     Sets merge.mergiraf in your global git config and adds a marked block to \
                     your global attributes file (also merge.conflictStyle = diff3, if you set \
                     none); unticking takes them out again.",
                    );
                if toggle.changed() {
                    mergiraf::use_in_merges(ui.ctx(), in_merges);
                }
            },
        );
    }

    /// One page of the settings, or what a search matches on it (see [`layout::Filter`]).
    fn page_ui(&mut self, ui: &mut Ui, page: Page, f: &Filter, view: &mut SettingsView) {
        let t = Tokens::get(ui.ctx());
        match page {
            Page::Profile => self.profile_ui(ui, f, view),
            Page::General => {
                section(ui, f, "Behaviour", None, |card| {
                    card.row("Worktrees", "open same tab switch", |ui| {
                        ui.checkbox(&mut self.worktrees_in_same_tab, "Open in the same tab")
                            .on_hover_text("The tab moves to the worktree and shows its branch, which also opens a list to switch between them. What you leave behind keeps running: terminals, operations, a half-written commit message.");
                    });
                    card.row("Notifications", "desktop long operation background", |ui| {
                        ui.checkbox(
                            &mut self.notifications,
                            "When a long operation ends in the background",
                        );
                    });
                    file_menu_rows(card);
                    card.row("Tips", "did you know hints help first time", |ui| {
                        self.tips_ui(ui);
                    });
                    if let Some(appimage) =
                        crate::desktop_entry::appimage().filter(|_| !crate::portable::asked())
                    {
                        card.row(
                            "Applications menu",
                            "appimage desktop entry launcher",
                            |ui| {
                                if crate::desktop_entry::installed() {
                                    if ui.button("Remove gitcraft").clicked() {
                                        let _ = crate::desktop_entry::uninstall();
                                    }
                                } else if ui.button("Add gitcraft").clicked() {
                                    let _ = crate::desktop_entry::install(&appimage);
                                }
                                layout::note(
                                    ui,
                                    "Starts this AppImage; follows it when you replace it",
                                );
                            },
                        );
                    }
                });
                self.portable_ui(ui, f);
                section(ui, f, "Updates", None, |card| {
                    card.row("Version", "about release", |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!("gitcraft {}", crate::update::CURRENT))
                                .color(t.text_dim),
                        );
                    });
                    card.row("Check now", "check for updates new version look", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let checking = view.update == UpdateCheck::Checking;
                            let pinned = matches!(view.update, UpdateCheck::Pinned(_));
                            if ui
                                .add_enabled(
                                    !checking && !pinned,
                                    egui::Button::new("Check for updates"),
                                )
                                .on_hover_text(format!(
                                    "Looks on GitHub for a newer version on the {} channel",
                                    self.update_channel.label()
                                ))
                                .on_disabled_hover_text(if pinned {
                                    "Pinned to this version: unpin it to look for updates"
                                } else {
                                    "Looking…"
                                })
                                .clicked()
                            {
                                view.check_updates_now = true;
                            }
                            match &view.update {
                                UpdateCheck::NotYet => {}
                                UpdateCheck::Checking => {
                                    ui.add(crate::spinner::Spinner::new());
                                    layout::note(ui, "Looking…");
                                }
                                UpdateCheck::UpToDate => {
                                    layout::note(ui, "This is the newest version");
                                }
                                UpdateCheck::Found(version) => {
                                    ui.label(
                                        RichText::new(format!(
                                            "gitcraft {version} is out: see the bar at the top"
                                        ))
                                        .color(t.success)
                                        .small(),
                                    );
                                }
                                UpdateCheck::Failed(e) => {
                                    ui.label(RichText::new(e).color(t.danger).small());
                                }
                                // Said in the Pinned row, with its Unpin button.
                                UpdateCheck::Pinned(_) => {}
                            }
                        });
                    });
                    rollback_rows(card, view);
                    card.row("New versions", "check download github restart", |ui| {
                        ui.checkbox(
                            &mut self.check_updates,
                            "Look for a new version on GitHub on every start",
                        );
                        ui.add_enabled_ui(self.check_updates, |ui| {
                            ui.checkbox(
                                &mut self.download_updates,
                                "Download it in the background, ready to install with a restart",
                            );
                        });
                    });
                    card.row("Channel", "main dev releases builds", |ui| {
                        ui.add_enabled_ui(self.check_updates, |ui| {
                            egui::ComboBox::from_id_salt("settings-update-channel")
                                .width(layout::fit(ui, COMBO_WIDTH))
                                .selected_text(self.update_channel.label())
                                .show_ui(ui, |ui| {
                                    for channel in crate::update::Channel::ALL {
                                        ui.selectable_value(
                                            &mut self.update_channel,
                                            channel,
                                            channel.label(),
                                        );
                                    }
                                });
                        });
                    });
                });
            }
            Page::Appearance => {
                section(
                    ui,
                    f,
                    "Theme",
                    Some("Each profile has its own theme; the accent and corners are for all."),
                    |card| self.themes_ui(card, f, view),
                );
                section(ui, f, "Appearance", None, |card| {
                    card.row("Zoom", "scale size interface ui", |ui| {
                        ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
                        // Zooming moves the slider under the pointer, which would move the value
                        // again: while it is dragged, the value waits aside and is applied when
                        // it is let go. A click or the arrow keys apply it at once.
                        let pending = egui::Id::new("settings-zoom-dragged");
                        let mut zoom = ui
                            .data(|d| d.get_temp::<f32>(pending))
                            .unwrap_or(self.ui_scale);
                        let slider = ui.add(layout::slider(
                            egui::Slider::new(&mut zoom, 0.7..=2.0)
                                .step_by(0.05)
                                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                        ));
                        if slider.dragged() {
                            ui.data_mut(|d| d.insert_temp(pending, zoom));
                        } else {
                            ui.data_mut(|d| d.remove::<f32>(pending));
                            if slider.changed() || slider.drag_stopped() {
                                self.ui_scale = zoom;
                            }
                        }
                    });
                    if cfg!(any(windows, target_os = "macos")) {
                        card.row("Title bar", "window frame caption native tabs", |ui| {
                            ui.checkbox(&mut self.tabs_in_title_bar, "Tabs in the title bar")
                                .on_hover_text("The tabs take the place of the system's title bar, as in a browser: drag the space next to them to move the window, double-click it to maximize, and the buttons to minimize, maximize and close are at the end of the strip.");
                        });
                    }
                    for button in crate::app_buttons::AppButton::ALL {
                        let keywords = "button place move title bar bottom footer panel";
                        card.row(button.label(), keywords, |ui| {
                            let layout = self.button_layout.normalized();
                            let was = layout.place_of(button);
                            let mut place = was;
                            egui::ComboBox::from_id_salt(("settings-button-place", button.label()))
                                .width(layout::fit(ui, COMBO_WIDTH))
                                .selected_text(place.label())
                                .show_ui(ui, |ui| {
                                    for p in crate::app_buttons::Place::ALL {
                                        ui.selectable_value(&mut place, p, p.label());
                                    }
                                })
                                .response
                                .on_hover_text(
                                    "Where its button sits: in the title bar, or in the footer \
                                     of a repository's left or right panel. Drag the button \
                                     itself to put it anywhere along them.",
                                );
                            if place != was {
                                self.button_layout = layout.moved(button, place, usize::MAX);
                            }
                        });
                    }
                    card.preview(Preview::CodeColors);
                    card.row(
                        "Code colours",
                        "vs code vscode theme syntax highlighting colors",
                        |ui| {
                            ui.checkbox(&mut self.vscode_colors, "Use VS Code's colour theme")
                                .on_hover_text(
                                    "Colours code in diffs and blame like your editor does",
                                );
                            if self.vscode_colors {
                                let note = match crate::vscode_theme::current() {
                                    Some(found) if found.dark == t.dark() => found.name.clone(),
                                    Some(found) => format!(
                                        "{} is {}: gitcraft's colours meanwhile",
                                        found.name,
                                        if found.dark { "dark" } else { "light" }
                                    ),
                                    None => "No VS Code theme found".to_owned(),
                                };
                                indented_note(ui, note);
                            }
                        },
                    );
                    card.preview(Preview::Avatars);
                    card.row("Avatars", "github gitlab forgejo gitea forge gravatar pictures authors", |ui| {
                        ui.checkbox(&mut self.avatars, "Load from the forge or Gravatar")
                            .on_hover_text("On a GitHub, Forgejo or Gitea repository, its commit list says which account each author is; on GitLab, the host is asked by email. Other authors: an MD5 hash of the email goes to gravatar.com. Cached on disk.");
                    });
                });
                section(ui, f, "Diffs and files", None, |card| {
                    card.preview(Preview::DiffLayout);
                    card.row("Diff layout", "unified split side by side columns", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            ui.radio_value(&mut self.diff_split, false, "Unified");
                            ui.radio_value(&mut self.diff_split, true, "Side by side");
                        });
                    });
                    card.preview(Preview::DiffContext);
                    card.row(
                        "Diff context",
                        "whole file lines around changes hunks fold",
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 18.0;
                                ui.radio_value(
                                    &mut self.diff_whole_file,
                                    false,
                                    "Around the changes",
                                );
                                ui.radio_value(&mut self.diff_whole_file, true, "The whole file");
                            });
                        },
                    );
                    card.preview(Preview::FileList);
                    card.row("Changed files", "tree folders list flat", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            ui.radio_value(&mut self.files_as_tree, true, "In folders");
                            ui.radio_value(&mut self.files_as_tree, false, "One list");
                        });
                    });
                });
            }
            Page::Graph => {
                section(ui, f, "History filters", None, |card| {
                    card.row(
                        "Global history filters",
                        "authors names emails namespaces T3 Code hide exclude",
                        |ui| {
                            crate::repo::history_filter::edit(ui, &mut self.history_filter);
                        },
                    );
                });
                section(ui, f, "Commit graph", None, |card| {
                    card.preview(Preview::GraphRefs);
                    card.row("Branch column width", "refs tags pills", |ui| {
                        ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
                        if ui
                            .add(layout::slider(
                                egui::Slider::new(
                                    &mut self.refs_width,
                                    crate::repo::columns::MIN_REFS_W..=2000.0,
                                )
                                .step_by(10.0)
                                .suffix(" px"),
                            ))
                            .changed()
                        {
                            self.auto_refs_width = false;
                        }
                        let mut fit = self.fit_visible_refs();
                        if ui.checkbox(&mut fit, "Fit visible refs").changed() {
                            self.auto_refs_width = fit;
                            if fit {
                                self.refs_width = DEFAULT_REFS_WIDTH;
                            }
                        }
                    });
                    card.preview(Preview::GraphRefs);
                    card.row(
                        "Shortened branch names",
                        "refs tags pills abbreviation ellipsis suffix prefix",
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 18.0;
                                ui.radio_value(
                                    &mut self.branch_names_keep_end,
                                    true,
                                    "Keep end (…xyz)",
                                );
                                ui.radio_value(
                                    &mut self.branch_names_keep_end,
                                    false,
                                    "Keep start (xyz…)",
                                );
                            });
                        },
                    );
                    card.preview(Preview::GraphColumns);
                    card.row_hint(
                        "Columns",
                        "Or right-click the graph's header",
                        "author when date hash tests show",
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 18.0;
                                ui.checkbox(&mut self.show_author, "Author");
                                ui.checkbox(&mut self.show_when, "When");
                                ui.checkbox(&mut self.show_date, "Date");
                                ui.checkbox(&mut self.show_hash, "Hash");
                                ui.checkbox(&mut self.show_test_lane, "Tests");
                            });
                        },
                    );
                    card.row("Diffs", "diff split below beside graph", |ui| {
                        ui.checkbox(&mut self.diff_beside_graph, "Show diffs below the graph")
                            .on_hover_text("A diff of a commit or of the working tree opens below the graph instead of in its place; the button in a diff's header switches it too.");
                    });
                    card.preview(Preview::GraphLanes);
                    card.row("Lanes", "compact graph narrow", |ui| {
                        let mut compact = self.compact_graph;
                        if ui.checkbox(&mut compact, "Compact graph").changed() {
                            self.set_compact_graph(compact);
                        }
                        indented_note(ui, COMPACT_TIP);
                    });
                    card.preview(Preview::LaneColors);
                    card.row(
                        "Lane colours",
                        "colors colour blind palette okabe ito classic theme",
                        |ui| {
                            egui::ComboBox::from_id_salt("settings-lane-colors")
                                .width(layout::fit(ui, COMBO_WIDTH))
                                .selected_text(self.theme_tweaks.lanes.label())
                                .show_ui(ui, |ui| {
                                    for option in crate::theme::LaneColors::ALL {
                                        ui.selectable_value(
                                            &mut self.theme_tweaks.lanes,
                                            option,
                                            option.label(),
                                        );
                                    }
                                });
                        },
                    );
                });
            }
            Page::Git => {
                section(ui, f, "Git", None, |card| {
                    card.row(
                        "Fetch in the background",
                        "auto fetch interval minutes remote",
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.auto_fetch_minutes)
                                        .range(0..=240)
                                        .suffix(" min"),
                                );
                                layout::note(ui, "0 turns it off");
                            });
                        },
                    );
                    card.preview(Preview::Pull);
                    card.row("Offline", "offline metered network data roaming", |ui| {
                        ui.checkbox(&mut self.offline, "Offline mode")
                            .on_hover_text(OFFLINE_TIP);
                        ui.checkbox(
                            &mut self.pause_on_metered,
                            "Pause background work on a metered connection",
                        )
                        .on_hover_text(METERED_TIP);
                        if let Some(pause) = crate::network::background_paused() {
                            indented_note(
                                ui,
                                format!("{}: background work is paused.", pause.why()),
                            );
                        }
                    });
                    card.row("Pull", "fast-forward rebase merge", |ui| {
                        egui::ComboBox::from_id_salt("settings-pull")
                            .width(layout::fit(ui, COMBO_WIDTH))
                            .selected_text(self.pull_mode.label())
                            .show_ui(ui, |ui| {
                                for mode in crate::repo::pull::PullMode::ALL {
                                    ui.selectable_value(&mut self.pull_mode, mode, mode.label());
                                }
                            });
                    });
                    card.row("Submodules", "update recursive", |ui| {
                        ui.checkbox(&mut self.keep_submodules, "Keep submodules up to date")
                            .on_hover_text(
                                "After a checkout, pull, merge or rebase in a repository with \
                                 submodules, run git submodule update --init --recursive, so \
                                 each one is at the commit the repository now records.",
                            );
                    });
                    card.row(
                        "Stale branches",
                        "stale branch cleanup old days merged delete",
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.stale_branch_days)
                                        .range(0..=3650)
                                        .suffix(" days"),
                                )
                                .on_hover_text(
                                    "A local branch with no commit for this long is marked \
                                     Stale in the sidebar and offered by Select stale or \
                                     merged branches…",
                                );
                                layout::note(ui, "0 marks none");
                            });
                        },
                    );
                    card.row_hint(
                        "Editor command",
                        "Opens a folder or file",
                        "code vs code subl idea open",
                        |ui| {
                            ui.add(
                                layout::field(&mut self.editor)
                                    .hint_text("code")
                                    .desired_width(layout::fit(ui, FIELD_WIDTH)),
                            )
                            .on_hover_text("Run with a folder or file path, e.g. code, subl, idea");
                        },
                    );
                });
                section(ui, f, "Diffs", None, |card| self.diffs_ui(card));
                section(
                    ui,
                    f,
                    "External tools",
                    Some(external_tools::INTRO),
                    |card| self.external_tools_ui(card),
                );
                section(
                    ui,
                    f,
                    "Mergiraf",
                    Some("Solves merge conflicts from the syntax of the code."),
                    |card| self.mergiraf_ui(card),
                );
                section(
                    ui,
                    f,
                    "Remote machines",
                    Some(
                        "Repositories in WSL or over SSH run git there, with that machine's own \
                         git config: the settings above that write this machine's global git \
                         config, and a profile's author and signing, don't reach it.",
                    ),
                    |card| {
                        card.row_hint(
                            "SSH sign-ins",
                            "Kept with \"Remember connection\"",
                            "ssh password passphrase forget saved credential keyring wsl remote",
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    // The system's store can take a while (the Secret Service
                                    // is another process): not on the window's thread.
                                    if ui
                                        .button("Forget saved sign-ins")
                                        .on_hover_text(
                                            "Takes every SSH password and passphrase gitcraft \
                                             saved out of the system's credential store",
                                        )
                                        .clicked()
                                    {
                                        std::thread::spawn(crate::remote::askpass::forget_saved);
                                    }
                                });
                            },
                        );
                    },
                );
            }
            Page::Ai => self.ai_ui(ui, f),
            Page::Signing => {
                let profile = self.profile();
                let overridden = profile.own_signing.then(|| profile.title().to_owned());
                signing::signing_ui(ui, f, overridden.as_deref());
            }
            Page::Terminal => {
                section(ui, f, "Terminal", None, |card| {
                    card.preview(Preview::Terminal);
                    card.row("Default shell", "bash zsh powershell cmd fish", |ui| {
                        let shells = crate::terminal::available_shells();
                        let current = shells
                            .iter()
                            .find(|s| s.program == self.shell)
                            .map(|s| s.label.clone())
                            .unwrap_or_else(|| "System default".to_owned());
                        egui::ComboBox::from_id_salt("settings-shell")
                            .width(layout::fit(ui, COMBO_WIDTH))
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.shell,
                                    String::new(),
                                    "System default",
                                );
                                for s in &shells {
                                    ui.selectable_value(
                                        &mut self.shell,
                                        s.program.clone(),
                                        &s.label,
                                    );
                                }
                            });
                    });
                    card.preview(Preview::Terminal);
                    card.row("Font size", "text", |ui| {
                        ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
                        ui.add(layout::slider(
                            egui::Slider::new(&mut self.terminal_font, 9.0..=20.0)
                                .step_by(0.5)
                                .max_decimals(1)
                                .suffix(" pt"),
                        ));
                    });
                });
            }
            Page::Identities => {
                section(
                    ui,
                    f,
                    "Identities",
                    Some(
                        "Name and email pairs you can apply to a repository from its commit box. \
                         That sets them in the repository's own config, where they win over the \
                         profile's author.",
                    ),
                    |card| {
                        let mut remove = None;
                        for (i, p) in self.identities.iter_mut().enumerate() {
                            let found_by = format!("{} {} {}", p.label, p.name, p.email);
                            card.full(&found_by, |ui| {
                                ui.horizontal(|ui| {
                                    // Shares of the row, less the gaps and the remove button.
                                    let room = ui.available_width() - 60.0;
                                    ui.add(
                                        layout::field(&mut p.label)
                                            .hint_text("Work")
                                            .desired_width((room * 0.2).min(110.0)),
                                    );
                                    ui.add(
                                        layout::field(&mut p.name)
                                            .hint_text("Name")
                                            .desired_width((room * 0.32).min(170.0)),
                                    );
                                    // The email takes what is left, the remove button at the end.
                                    let rest = (ui.available_width() - 32.0).max(60.0);
                                    ui.add(
                                        layout::field(&mut p.email)
                                            .hint_text("email@example.com")
                                            .desired_width(rest),
                                    );
                                    if crate::icons::small_button(ui, "x", "Remove").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
                        }
                        if let Some(i) = remove {
                            self.identities.remove(i);
                        }
                        card.full("add new name email user profile", |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Add identity").clicked() {
                                    self.identities.push(SavedIdentity::default());
                                }
                                if self.identities.is_empty() {
                                    layout::note(ui, "None yet");
                                }
                            });
                        });
                    },
                );
            }
            Page::CustomCommands => custom_commands::page(self, ui, f),
            Page::Agents => self.agents_ui(ui, f),
            Page::ContextMenus => self.context_menus_ui(ui, f),
            Page::Shortcuts => {
                for (group, keys) in crate::manual::shortcut_groups() {
                    section(ui, f, group, None, |card| {
                        card.compact();
                        for (combo, what) in &keys {
                            card.full(&format!("{combo} {what} keys keyboard"), |ui| {
                                ui.horizontal(|ui| {
                                    let width = layout::label_width(ui);
                                    let start = ui.cursor().left();
                                    crate::manual::key_combo(ui, combo);
                                    let used = ui.cursor().left() - start;
                                    ui.add_space((width + 8.0 - used).max(8.0));
                                    ui.label(RichText::new(*what).color(t.text_dim));
                                });
                            });
                        }
                    });
                }
            }
        }
    }
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

/// The Updates section's rows for going back to the version the last update replaced, and for
/// the pin that keeps it.
fn rollback_rows(card: &mut Card, view: &mut SettingsView) {
    if let Some(version) = view.previous.clone() {
        let (button, tip) = if view.previous_is_newer {
            (
                format!("Go forward to {version}"),
                format!(
                    "Restarts as gitcraft {version}, the newer version kept when you went back; \
                     the open tabs stay, and updates are offered again."
                ),
            )
        } else {
            (
                format!("Go back to {version}"),
                format!(
                    "Restarts as {version}, kept by the last update; the open tabs stay. No \
                     update is offered until you unpin it."
                ),
            )
        };
        card.row(
            "Previous version",
            "go back forward rollback undo update downgrade",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button(button).on_hover_text(tip).clicked() {
                        view.go_back = true;
                    }
                });
            },
        );
    }
    if !view.pinned.is_empty() {
        let pinned = view.pinned.clone();
        card.row("Pinned", "pin stay version unpin", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Staying on gitcraft {pinned}"));
                if ui
                    .button("Unpin")
                    .on_hover_text("Look for new versions again, as the update settings say")
                    .clicked()
                {
                    view.unpin = true;
                }
            });
        });
    }
}

/// The pages of the settings, in the order of their tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    Profile,
    #[default]
    General,
    Appearance,
    Graph,
    Git,
    Signing,
    Terminal,
    Identities,
    Ai,
    CustomCommands,
    ContextMenus,
    Agents,
    Shortcuts,
}

impl Page {
    pub const ALL: [Page; 13] = [
        Page::Profile,
        Page::General,
        Page::Appearance,
        Page::Graph,
        Page::Git,
        Page::Signing,
        Page::Terminal,
        Page::Identities,
        Page::Ai,
        Page::CustomCommands,
        Page::ContextMenus,
        Page::Agents,
        Page::Shortcuts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Profile => "Profile",
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Graph => "Commit graph",
            Page::Git => "Git",
            Page::Signing => "Commit signing",
            Page::Terminal => "Terminal",
            Page::Identities => "Identities",
            Page::Ai => "AI",
            Page::CustomCommands => "Custom commands",
            Page::Agents => "Agents",
            Page::ContextMenus => "Context menus",
            Page::Shortcuts => "Keyboard shortcuts",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Profile => "circle-user-round",
            Page::General => "settings",
            Page::Appearance => "palette",
            Page::Graph => "git-branch",
            Page::Git => "git-merge",
            Page::Signing => "key-round",
            Page::Terminal => "square-terminal",
            Page::Identities => "user-round",
            Page::Ai => "sparkles",
            Page::CustomCommands => "command",
            Page::Agents => "bot",
            Page::ContextMenus => "list",
            Page::Shortcuts => "keyboard",
        }
    }
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
