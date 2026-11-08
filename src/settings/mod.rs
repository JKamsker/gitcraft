//! User settings: stored with the app state, readable from anywhere through the egui context.

mod agents;
mod ai_page;
mod branch_names;
mod connection_page;
mod context_menus;
mod custom_commands;
mod diffs;
mod external_tools;
mod file_menus;
mod labels;
mod language;
mod layout;
pub(crate) mod network_page;
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
            tabs_in_title_bar: false,
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

pub const COMPACT_TIP: &str = "Lanes closer together, for a narrower graph column";
/// Localized prose; the legacy constant stays stable for source consumers during migration.
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
                            crate::i18n::message(
                                "interaction-settings-mergiraf-version",
                                &[("version", found.version.as_str().into())],
                            )
                        } else {
                            found.version.clone()
                        };
                        ui.label(RichText::new(what).color(t.text_dim));
                    }
                    None => {
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(crate::i18n::text(
                                    "interaction-settings-install",
                                )),
                            )
                            .on_hover_text(crate::i18n::text(
                                "interaction-settings-mergiraf-download",
                            ))
                            .clicked()
                        {
                            mergiraf::install(ui.ctx());
                        }
                        ui.hyperlink_to(
                            crate::i18n::text("interaction-settings-other-ways"),
                            mergiraf::DOCS,
                        );
                        if matches!(job, Job::Idle) {
                            layout::note(
                                ui,
                                crate::i18n::text("interaction-settings-not-installed"),
                            );
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
                egui::Checkbox::new(
                    &mut self.mergiraf,
                    crate::i18n::text("interaction-settings-mergiraf-offer"),
                ),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-mergiraf-editor"));
        });
        card.row(
            "Git merges",
            "every merge rebase cherry-pick driver",
            |ui| {
                let mut in_merges = state.in_merges;
                let toggle = ui
                    .add_enabled(
                        state.found.is_some() && !busy,
                        egui::Checkbox::new(
                            &mut in_merges,
                            crate::i18n::text("interaction-settings-mergiraf-every-merge"),
                        ),
                    )
                    .on_hover_text(crate::i18n::text("interaction-settings-mergiraf-global"));
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
            Page::Network => self.network_ui(ui, f, false),
            Page::HttpsSignins => self.network_ui(ui, f, true),
            Page::General => {
                section(
                    ui,
                    f,
                    crate::i18n::text("settings-language-section"),
                    None,
                    |card| {
                        card.row(
                            crate::i18n::text("settings-language-label"),
                            "language locale translation Sprache",
                            |ui| {
                                self.language_ui(ui);
                            },
                        );
                    },
                );
                section(ui, f, "Quiet celebrations", None, |card| {
                    card.row(
                        "After success",
                        "first commit release tag merge sound motion",
                        |ui| {
                            ui.checkbox(
                                &mut self.celebration_motion,
                                crate::i18n::text("interaction-settings-celebration-motion"),
                            );
                            ui.checkbox(
                                &mut self.celebration_sound,
                                crate::i18n::text("interaction-settings-celebration-sound"),
                            );
                            ui.small(crate::i18n::text("interaction-settings-celebration-note"));
                            if crate::audio::unavailable() {
                                ui.label(crate::i18n::text(
                                    "interaction-settings-audio-unavailable",
                                ));
                                if ui
                                    .button(crate::i18n::text("interaction-settings-retry-audio"))
                                    .clicked()
                                {
                                    crate::audio::retry();
                                }
                            }
                        },
                    );
                });
                section(ui, f, "Behaviour", None, |card| {
                    card.row("Worktrees", "open same tab switch", |ui| {
                        ui.checkbox(
                            &mut self.worktrees_in_same_tab,
                            crate::i18n::text("interaction-settings-same-tab"),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-same-tab-tip"));
                    });
                    card.row("Notifications", "desktop long operation background", |ui| {
                        ui.checkbox(
                            &mut self.notifications,
                            crate::i18n::text("interaction-settings-notify-when"),
                        );
                    });
                    card.row(
                        "Resident watcher",
                        "tray background close recent repositories notifications",
                        |ui| {
                            ui.checkbox(
                                &mut self.keep_running_in_tray,
                                crate::i18n::text("interaction-settings-resident"),
                            );
                            ui.small(crate::i18n::text("interaction-settings-resident-policy"));
                            if self.keep_running_in_tray {
                                match crate::tray::status() {
                                    crate::tray::Availability::Ready => {
                                        ui.small(crate::i18n::text(
                                            "interaction-settings-tray-ready",
                                        ));
                                    }
                                    crate::tray::Availability::Fallback(why) => {
                                        ui.small(crate::i18n::message(
                                            "interaction-settings-tray-fallback",
                                            &[("why", why.as_str().into())],
                                        ));
                                    }
                                    _ => {
                                        ui.small(crate::i18n::text(
                                            "interaction-settings-tray-starting",
                                        ));
                                    }
                                }
                            }
                        },
                    );
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
                                    if ui
                                        .button(crate::i18n::text(
                                            "interaction-settings-remove-app",
                                        ))
                                        .clicked()
                                    {
                                        let _ = crate::desktop_entry::uninstall();
                                    }
                                } else if ui
                                    .button(crate::i18n::text("interaction-settings-add-app"))
                                    .clicked()
                                {
                                    let _ = crate::desktop_entry::install(&appimage);
                                }
                                layout::note(
                                    ui,
                                    crate::i18n::text("interaction-settings-appimage-note"),
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
                                    egui::Button::new(crate::i18n::text(
                                        "interaction-settings-check-updates",
                                    )),
                                )
                                .on_hover_text(crate::i18n::message(
                                    "interaction-settings-update-channel",
                                    &[("channel", self.update_channel.label().into())],
                                ))
                                .on_disabled_hover_text(if pinned {
                                    crate::i18n::text("interaction-settings-pinned-tip")
                                } else {
                                    crate::i18n::text("interaction-settings-looking")
                                })
                                .clicked()
                            {
                                view.check_updates_now = true;
                            }
                            match &view.update {
                                UpdateCheck::NotYet => {}
                                UpdateCheck::Checking => {
                                    ui.add(crate::spinner::Spinner::new());
                                    layout::note(
                                        ui,
                                        crate::i18n::text("interaction-settings-looking"),
                                    );
                                }
                                UpdateCheck::UpToDate => {
                                    layout::note(
                                        ui,
                                        crate::i18n::text("interaction-settings-newest"),
                                    );
                                }
                                UpdateCheck::Found(version) => {
                                    ui.label(
                                        RichText::new(crate::i18n::message(
                                            "interaction-settings-update-found",
                                            &[("version", version.as_str().into())],
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
                            crate::i18n::text("interaction-settings-updates-start"),
                        );
                        ui.add_enabled_ui(self.check_updates, |ui| {
                            ui.checkbox(
                                &mut self.download_updates,
                                crate::i18n::text("interaction-settings-updates-download"),
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
                    Some(crate::i18n::text("interaction-settings-theme-scope")),
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
                            ui.checkbox(
                                &mut self.tabs_in_title_bar,
                                crate::i18n::text("interaction-settings-tabs-title"),
                            )
                            .on_hover_text(crate::i18n::text(
                                "interaction-settings-tabs-title-tip",
                            ));
                        });
                    }
                    for button in crate::app_buttons::AppButton::ALL {
                        let keywords = "button place move title bar bottom footer panel";
                        // Row/search identities remain English; Card resolves the visible caption.
                        let row_name = match button {
                            crate::app_buttons::AppButton::Settings => "Settings",
                            crate::app_buttons::AppButton::Palette => "Command palette",
                            crate::app_buttons::AppButton::Theme => "Theme",
                            crate::app_buttons::AppButton::Manual => "Manual",
                        };
                        card.row(row_name, keywords, |ui| {
                            let layout = self.button_layout.normalized();
                            let was = layout.place_of(button);
                            let mut place = was;
                            egui::ComboBox::from_id_salt(("settings-button-place", button))
                                .width(layout::fit(ui, COMBO_WIDTH))
                                .selected_text(place.label())
                                .show_ui(ui, |ui| {
                                    for p in crate::app_buttons::Place::ALL {
                                        ui.selectable_value(&mut place, p, p.label());
                                    }
                                })
                                .response
                                .on_hover_text(crate::i18n::text(
                                    "interaction-settings-button-place",
                                ));
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
                            ui.checkbox(
                                &mut self.vscode_colors,
                                crate::i18n::text("interaction-settings-vscode-use"),
                            )
                            .on_hover_text(crate::i18n::text("interaction-settings-vscode-tip"));
                            if self.vscode_colors {
                                let note = match crate::vscode_theme::current() {
                                    Some(found) if found.dark == t.dark() => found.name.clone(),
                                    Some(found) => crate::i18n::message(
                                        if found.dark {
                                            "interaction-settings-vscode-dark"
                                        } else {
                                            "interaction-settings-vscode-light"
                                        },
                                        &[("theme", found.name.as_str().into())],
                                    ),
                                    None => {
                                        crate::i18n::text("interaction-settings-vscode-missing")
                                            .to_owned()
                                    }
                                };
                                indented_note(ui, note);
                            }
                        },
                    );
                    card.preview(Preview::Avatars);
                    card.row(
                        "Avatars",
                        "github gitlab forgejo gitea forge gravatar pictures authors",
                        |ui| {
                            ui.checkbox(
                                &mut self.avatars,
                                crate::i18n::text("interaction-settings-avatars-load"),
                            )
                            .on_hover_text(crate::i18n::text("interaction-settings-avatars-tip"));
                        },
                    );
                });
                section(ui, f, "Diffs and files", None, |card| {
                    card.preview(Preview::DiffLayout);
                    card.row("Diff layout", "unified split side by side columns", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            ui.radio_value(
                                &mut self.diff_split,
                                false,
                                crate::i18n::text("interaction-settings-unified"),
                            );
                            ui.radio_value(
                                &mut self.diff_split,
                                true,
                                crate::i18n::text("interaction-settings-side-by-side"),
                            );
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
                                    crate::i18n::text("interaction-settings-around-changes"),
                                );
                                ui.radio_value(
                                    &mut self.diff_whole_file,
                                    true,
                                    crate::i18n::text("interaction-settings-whole-file"),
                                );
                            });
                        },
                    );
                    card.preview(Preview::FileList);
                    card.row("Changed files", "tree folders list flat", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 18.0;
                            ui.radio_value(
                                &mut self.files_as_tree,
                                true,
                                crate::i18n::text("interaction-settings-folders"),
                            );
                            ui.radio_value(
                                &mut self.files_as_tree,
                                false,
                                crate::i18n::text("interaction-settings-one-list"),
                            );
                        });
                    });
                });
            }
            Page::Graph => {
                section(ui, f, "Graph feedback", None, |card| {
                    card.row(
                        "Explanatory motion",
                        "fetch incoming refs scroll reduced motion",
                        |ui| {
                            ui.checkbox(
                                &mut self.graph_motion,
                                crate::i18n::text("interaction-settings-ref-transitions"),
                            );
                            ui.small(crate::i18n::text(
                                "interaction-settings-reduced-motion-note",
                            ));
                        },
                    );
                });
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
                    card.row(
                        "Topology navigation",
                        "accessibility audio lanes graph parent child",
                        |ui| {
                            ui.checkbox(
                                &mut self.graph_lane_walk,
                                crate::i18n::text("interaction-settings-walk-topology"),
                            );
                            ui.checkbox(
                                &mut self.graph_sounds,
                                crate::i18n::text("interaction-settings-graph-sounds"),
                            );
                        },
                    );
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
                                .custom_formatter(|value, _| crate::i18n::number(value))
                                .custom_parser(parse_localized_number)
                                .suffix(" px"),
                            ))
                            .changed()
                        {
                            self.auto_refs_width = false;
                        }
                        let mut fit = self.fit_visible_refs();
                        if ui
                            .checkbox(&mut fit, crate::i18n::text("interaction-settings-fit-refs"))
                            .changed()
                        {
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
                                    crate::i18n::text("interaction-settings-keep-end"),
                                );
                                ui.radio_value(
                                    &mut self.branch_names_keep_end,
                                    false,
                                    crate::i18n::text("interaction-settings-keep-start"),
                                );
                            });
                        },
                    );
                    card.preview(Preview::GraphColumns);
                    card.row_hint(
                        "Columns",
                        crate::i18n::text("interaction-settings-columns-tip"),
                        "author when date hash tests show",
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 18.0;
                                ui.checkbox(
                                    &mut self.show_author,
                                    crate::i18n::text("interaction-settings-author"),
                                );
                                ui.checkbox(
                                    &mut self.show_when,
                                    crate::i18n::text("interaction-settings-when"),
                                );
                                ui.checkbox(
                                    &mut self.show_date,
                                    crate::i18n::text("interaction-settings-date"),
                                );
                                ui.checkbox(
                                    &mut self.show_hash,
                                    crate::i18n::text("interaction-settings-hash"),
                                );
                                ui.checkbox(
                                    &mut self.show_test_lane,
                                    crate::i18n::text("interaction-settings-tests"),
                                );
                            });
                        },
                    );
                    card.row("Diffs", "diff split below beside graph", |ui| {
                        ui.checkbox(
                            &mut self.diff_beside_graph,
                            crate::i18n::text("interaction-settings-diff-below"),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-diff-below-tip"));
                    });
                    card.preview(Preview::GraphLanes);
                    card.row("Lanes", "compact graph narrow", |ui| {
                        let mut compact = self.compact_graph;
                        if ui
                            .checkbox(
                                &mut compact,
                                crate::i18n::text("interaction-settings-compact"),
                            )
                            .changed()
                        {
                            self.set_compact_graph(compact);
                        }
                        indented_note(ui, compact_tip());
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
                                        .custom_formatter(|value, _| crate::i18n::number(value))
                                        .custom_parser(parse_localized_number)
                                        .suffix(" min"),
                                );
                                layout::note(
                                    ui,
                                    crate::i18n::text("interaction-settings-fetch-off"),
                                );
                            });
                        },
                    );
                    card.preview(Preview::Pull);
                    card.row("Offline", "offline metered network data roaming", |ui| {
                        ui.checkbox(
                            &mut self.offline,
                            crate::i18n::text("interaction-settings-offline"),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-offline-tip"));
                        ui.checkbox(
                            &mut self.pause_on_metered,
                            crate::i18n::text("interaction-settings-metered"),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-metered-tip"));
                        if let Some(pause) = crate::network::background_paused() {
                            indented_note(
                                ui,
                                crate::i18n::message(
                                    "interaction-settings-network-paused",
                                    &[("reason", pause.why().into())],
                                ),
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
                        ui.checkbox(
                            &mut self.keep_submodules,
                            crate::i18n::text("interaction-settings-submodules-update"),
                        )
                        .on_hover_text(crate::i18n::text("interaction-settings-submodules-tip"));
                    });
                    card.row(
                        "Stale branches",
                        "stale branch cleanup old days merged delete",
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut self.stale_branch_days)
                                        .range(0..=3650)
                                        .custom_formatter(|value, _| crate::i18n::number(value))
                                        .custom_parser(parse_localized_number)
                                        .suffix(crate::i18n::text("interaction-settings-days")),
                                )
                                .on_hover_text(crate::i18n::text("interaction-settings-stale-tip"));
                                layout::note(
                                    ui,
                                    crate::i18n::text("interaction-settings-stale-off"),
                                );
                            });
                        },
                    );
                    card.row_hint(
                        "Editor command",
                        crate::i18n::text("interaction-settings-editor-hint"),
                        "code vs code subl idea open",
                        |ui| {
                            ui.add(
                                layout::field(&mut self.editor)
                                    .hint_text("code")
                                    .desired_width(layout::fit(ui, FIELD_WIDTH)),
                            )
                            .on_hover_text(crate::i18n::text("interaction-settings-editor-tip"));
                        },
                    );
                });
                section(ui, f, "Diffs", None, |card| self.diffs_ui(card));
                section(
                    ui,
                    f,
                    "External tools",
                    Some(external_tools::intro()),
                    |card| self.external_tools_ui(card),
                );
                section(
                    ui,
                    f,
                    "Mergiraf",
                    Some(crate::i18n::text("interaction-settings-mergiraf-intro")),
                    |card| self.mergiraf_ui(card),
                );
                section(
                    ui,
                    f,
                    "Git file tools",
                    Some(crate::i18n::text("interaction-settings-git-tools-intro")),
                    |card| {
                        card.row(
                            "Register gitcraft",
                            "difftool mergetool compare files standalone global config",
                            crate::standalone::registration::settings_ui,
                        );
                    },
                );
                section(
                    ui,
                    f,
                    "Remote machines",
                    Some(crate::i18n::text("interaction-settings-remote-intro")),
                    |card| {
                        card.row_hint(
                            "SSH sign-ins",
                            crate::i18n::text("interaction-settings-remember-hint"),
                            "ssh password passphrase forget saved credential keyring wsl remote",
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    // The system's store can take a while (the Secret Service
                                    // is another process): not on the window's thread.
                                    if ui
                                        .button(crate::i18n::text(
                                            "interaction-settings-forget-logins",
                                        ))
                                        .on_hover_text(crate::i18n::text(
                                            "interaction-settings-forget-tip",
                                        ))
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
                            .unwrap_or_else(|| {
                                crate::i18n::text("interaction-settings-system-default").to_owned()
                            });
                        egui::ComboBox::from_id_salt("settings-shell")
                            .width(layout::fit(ui, COMBO_WIDTH))
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.shell,
                                    String::new(),
                                    crate::i18n::text("interaction-settings-system-default"),
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
                                .custom_formatter(|value, _| crate::i18n::number(value))
                                .custom_parser(parse_localized_number)
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
                    Some(crate::i18n::text("interaction-settings-identities-intro")),
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
                                            .hint_text(crate::i18n::text(
                                                "interaction-settings-work-hint",
                                            ))
                                            .desired_width((room * 0.2).min(110.0)),
                                    );
                                    ui.add(
                                        layout::field(&mut p.name)
                                            .hint_text(crate::i18n::text(
                                                "interaction-settings-name-hint",
                                            ))
                                            .desired_width((room * 0.32).min(170.0)),
                                    );
                                    // The email takes what is left, the remove button at the end.
                                    let rest = (ui.available_width() - 32.0).max(60.0);
                                    ui.add(
                                        layout::field(&mut p.email)
                                            .hint_text("email@example.com")
                                            .desired_width(rest),
                                    );
                                    if crate::icons::small_button(
                                        ui,
                                        "x",
                                        crate::i18n::text("interaction-settings-remove"),
                                    )
                                    .clicked()
                                    {
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
                                if ui
                                    .button(crate::i18n::text("interaction-settings-add-identity"))
                                    .clicked()
                                {
                                    self.identities.push(SavedIdentity::default());
                                }
                                if self.identities.is_empty() {
                                    layout::note(
                                        ui,
                                        crate::i18n::text("interaction-settings-none-yet"),
                                    );
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
                crate::keymap::editor::page(&mut self.key_bindings, ui);
            }
        }
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

/// The Updates section's rows for going back to the version the last update replaced, and for
/// the pin that keeps it.
fn rollback_rows(card: &mut Card, view: &mut SettingsView) {
    if let Some(version) = view.previous.clone() {
        let (button, tip) = if view.previous_is_newer {
            (
                crate::i18n::message(
                    "interaction-settings-forward",
                    &[("version", version.as_str().into())],
                ),
                crate::i18n::message(
                    "interaction-settings-forward-tip",
                    &[("version", version.as_str().into())],
                ),
            )
        } else {
            (
                crate::i18n::message(
                    "interaction-settings-back",
                    &[("version", version.as_str().into())],
                ),
                crate::i18n::message(
                    "interaction-settings-back-tip",
                    &[("version", version.as_str().into())],
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
                ui.label(crate::i18n::message(
                    "interaction-settings-staying",
                    &[("version", pinned.as_str().into())],
                ));
                if ui
                    .button(crate::i18n::text("interaction-settings-unpin"))
                    .on_hover_text(crate::i18n::text("interaction-settings-unpin-tip"))
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
    Network,
    HttpsSignins,
    Terminal,
    Identities,
    Ai,
    CustomCommands,
    ContextMenus,
    Agents,
    Shortcuts,
}

impl Page {
    pub const ALL: [Page; 15] = [
        Page::Profile,
        Page::General,
        Page::Appearance,
        Page::Graph,
        Page::Git,
        Page::Signing,
        Page::Network,
        Page::HttpsSignins,
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
            Page::Network => "Network",
            Page::HttpsSignins => "HTTPS sign-ins",
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
            Page::Network => "network",
            Page::HttpsSignins => "key-round",
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
