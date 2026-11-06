//! User settings: stored with the app state, readable from anywhere through the egui context.

mod layout;
mod page;
mod signing;

use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::theme::Tokens;
use layout::{Card, Filter, section};

/// A name and email to commit with, applied per repository.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
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
    /// Width of the graph column; 0 = fit the lanes of the repository.
    pub graph_width: f32,
    /// The graph's lanes closer together, for a narrower graph column.
    pub compact_graph: bool,
    /// Widths of the author, when, date and hash columns (dragged in the graph header).
    pub author_width: f32,
    pub when_width: f32,
    pub date_width: f32,
    pub hash_width: f32,
    /// The column bar dragged last: that column keeps its width while the others make room.
    pub last_resized: Option<crate::repo::columns::Bar>,
    /// How far that bar was pulled past its column's minimum, shrinking the columns right of it.
    pub column_push: f32,
    pub show_author: bool,
    /// How long ago each commit was made ("3 hours ago").
    pub show_when: bool,
    /// An absolute date column.
    pub show_date: bool,
    pub show_hash: bool,
    /// The branches panel on the left of a repository tab.
    pub show_branches: bool,
    /// The commit detail / changes panel on the right of a repository tab.
    pub show_detail: bool,
    /// A worktree opens in the tab it was opened from, which then shows its branch.
    pub worktrees_in_same_tab: bool,
    /// Changed files in a tree of their folders rather than a flat list.
    pub files_as_tree: bool,
    /// Diffs side by side rather than unified.
    pub diff_split: bool,
    /// Diffs show the whole file, not just the lines around the changes.
    pub diff_whole_file: bool,
    /// Background fetch interval in minutes; 0 turns it off.
    pub auto_fetch_minutes: u32,
    /// What the toolbar's Pull does.
    pub pull_mode: crate::repo::pull::PullMode,
    /// Load avatars: GitHub / GitLab accounts on repositories hosted there, Gravatar otherwise.
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
    /// Offer Mergiraf in the conflict editor, when it is installed.
    pub mergiraf: bool,
    /// After a checkout, pull, merge or rebase, update the submodules to the commits it records.
    pub keep_submodules: bool,
    pub profiles: Vec<Profile>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            terminal_font: 12.5,
            shell: String::new(),
            refs_width: 280.0,
            graph_width: 0.0,
            compact_graph: false,
            author_width: 150.0,
            when_width: 100.0,
            date_width: 118.0,
            hash_width: 70.0,
            last_resized: None,
            column_push: 0.0,
            show_author: true,
            show_when: true,
            show_date: false,
            show_hash: false,
            show_branches: true,
            show_detail: true,
            worktrees_in_same_tab: true,
            files_as_tree: true,
            diff_split: false,
            diff_whole_file: false,
            auto_fetch_minutes: 10,
            pull_mode: Default::default(),
            avatars: true,
            editor: "code".to_owned(),
            notifications: true,
            check_updates: true,
            update_channel: Default::default(),
            download_updates: true,
            vscode_colors: true,
            mergiraf: true,
            keep_submodules: false,
            profiles: Vec::new(),
        }
    }
}

fn id() -> egui::Id {
    egui::Id::new("gitcraft-settings")
}

/// What "Compact graph" does.
pub const COMPACT_TIP: &str = "Lanes closer together, for a narrower graph column";

impl Settings {
    pub fn get(ctx: &egui::Context) -> Settings {
        ctx.data(|d| d.get_temp::<Settings>(id()))
            .unwrap_or_default()
    }

    /// Gives the graph's columns their first widths back; which of them show stays.
    pub fn reset_columns(&mut self) {
        let fresh = Settings::default();
        self.refs_width = fresh.refs_width;
        self.graph_width = fresh.graph_width;
        self.author_width = fresh.author_width;
        self.when_width = fresh.when_width;
        self.date_width = fresh.date_width;
        self.hash_width = fresh.hash_width;
        self.last_resized = fresh.last_resized;
        self.column_push = fresh.column_push;
    }

    /// Puts the graph's lanes closer together or back apart. The graph column then fits them
    /// again, or it would stay as wide.
    pub fn set_compact_graph(&mut self, compact: bool) {
        self.compact_graph = compact;
        self.graph_width = 0.0;
    }

    pub fn set(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(id(), self.clone()));
        ctx.set_zoom_factor(self.ui_scale.clamp(0.7, 2.0));
        crate::vscode_theme::set_enabled(self.vscode_colors);
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
                     repository and in the terminal too, so fewer conflicts are left at all. \
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
    fn page_ui(&mut self, ui: &mut Ui, page: Page, f: &Filter) {
        let t = Tokens::get(ui.ctx());
        match page {
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
                    if let Some(appimage) = crate::desktop_entry::appimage() {
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
                section(ui, f, "Updates", None, |card| {
                    card.row("Version", "about release", |ui| {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(format!("gitcraft {}", crate::update::CURRENT))
                                .color(t.text_dim),
                        );
                    });
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
                                .width(COMBO_WIDTH)
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
                section(ui, f, "Appearance", None, |card| {
                    card.row("Zoom", "scale size interface ui", |ui| {
                        ui.spacing_mut().slider_width = SLIDER_WIDTH;
                        ui.add(layout::slider(
                            egui::Slider::new(&mut self.ui_scale, 0.7..=2.0)
                                .step_by(0.05)
                                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                        ));
                    });
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
                    card.row("Avatars", "github gitlab gravatar pictures authors", |ui| {
                        ui.checkbox(&mut self.avatars, "Load from GitHub, GitLab or Gravatar")
                            .on_hover_text("On a GitHub repository, its commit list says which account each author is; on GitLab, the host is asked by email. Other authors: an MD5 hash of the email goes to gravatar.com. Cached on disk.");
                    });
                });
            }
            Page::Graph => {
                section(ui, f, "Commit graph", None, |card| {
                    card.row("Branch column width", "refs tags pills", |ui| {
                        ui.spacing_mut().slider_width = SLIDER_WIDTH;
                        ui.add(layout::slider(
                            egui::Slider::new(&mut self.refs_width, 210.0..=2000.0)
                                .step_by(10.0)
                                .suffix(" px"),
                        ));
                    });
                    card.row_hint(
                        "Columns",
                        "Or right-click the graph's header",
                        "author when date hash show",
                        |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = 18.0;
                                ui.checkbox(&mut self.show_author, "Author");
                                ui.checkbox(&mut self.show_when, "When");
                                ui.checkbox(&mut self.show_date, "Date");
                                ui.checkbox(&mut self.show_hash, "Hash");
                            });
                        },
                    );
                    card.row("Lanes", "compact graph narrow", |ui| {
                        let mut compact = self.compact_graph;
                        if ui.checkbox(&mut compact, "Compact graph").changed() {
                            self.set_compact_graph(compact);
                        }
                        indented_note(ui, COMPACT_TIP);
                    });
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
                    card.row("Pull", "fast-forward rebase merge", |ui| {
                        egui::ComboBox::from_id_salt("settings-pull")
                            .width(COMBO_WIDTH)
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
                    card.row_hint(
                        "Editor command",
                        "Opens a folder or file",
                        "code vs code subl idea open",
                        |ui| {
                            ui.add(
                                layout::field(&mut self.editor)
                                    .hint_text("code")
                                    .desired_width(FIELD_WIDTH),
                            )
                            .on_hover_text("Run with a folder or file path, e.g. code, subl, idea");
                        },
                    );
                });
                section(
                    ui,
                    f,
                    "Mergiraf",
                    Some("Solves merge conflicts from the syntax of the code."),
                    |card| self.mergiraf_ui(card),
                );
            }
            Page::Signing => signing::signing_ui(ui, f),
            Page::Terminal => {
                section(ui, f, "Terminal", None, |card| {
                    card.row("Default shell", "bash zsh powershell cmd fish", |ui| {
                        let shells = crate::terminal::available_shells();
                        let current = shells
                            .iter()
                            .find(|s| s.program == self.shell)
                            .map(|s| s.label.clone())
                            .unwrap_or_else(|| "System default".to_owned());
                        egui::ComboBox::from_id_salt("settings-shell")
                            .width(COMBO_WIDTH)
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
                    card.row("Font size", "text", |ui| {
                        ui.spacing_mut().slider_width = SLIDER_WIDTH;
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
                    Some("Name and email pairs you can apply to a repository from its commit box."),
                    |card| {
                        let mut remove = None;
                        for (i, p) in self.profiles.iter_mut().enumerate() {
                            let found_by = format!("{} {} {}", p.label, p.name, p.email);
                            card.full(&found_by, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        layout::field(&mut p.label)
                                            .hint_text("Work")
                                            .desired_width(110.0),
                                    );
                                    ui.add(
                                        layout::field(&mut p.name)
                                            .hint_text("Name")
                                            .desired_width(170.0),
                                    );
                                    // The email takes what is left, the remove button at the end.
                                    let rest = (ui.available_width() - 32.0).max(120.0);
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
                            self.profiles.remove(i);
                        }
                        card.full("add new name email user profile", |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Add identity").clicked() {
                                    self.profiles.push(Profile::default());
                                }
                                if self.profiles.is_empty() {
                                    layout::note(ui, "None yet");
                                }
                            });
                        });
                    },
                );
            }
            Page::Shortcuts => {
                for (group, keys) in crate::manual::SHORTCUTS {
                    section(ui, f, group, None, |card| {
                        card.compact();
                        for (combo, what) in *keys {
                            let combo = combo.replace("Cmd", crate::manual::command());
                            card.full(&format!("{combo} {what} keys keyboard"), |ui| {
                                ui.horizontal(|ui| {
                                    let start = ui.cursor().left();
                                    crate::manual::key_combo(ui, &combo);
                                    let used = ui.cursor().left() - start;
                                    ui.add_space((layout::LABEL_WIDTH + 8.0 - used).max(8.0));
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

/// The pages of the settings, in the order of their tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    General,
    Appearance,
    Graph,
    Git,
    Signing,
    Terminal,
    Identities,
    Shortcuts,
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::General,
        Page::Appearance,
        Page::Graph,
        Page::Git,
        Page::Signing,
        Page::Terminal,
        Page::Identities,
        Page::Shortcuts,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Graph => "Commit graph",
            Page::Git => "Git",
            Page::Signing => "Commit signing",
            Page::Terminal => "Terminal",
            Page::Identities => "Identities",
            Page::Shortcuts => "Keyboard shortcuts",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::General => "settings",
            Page::Appearance => "palette",
            Page::Graph => "git-branch",
            Page::Git => "git-merge",
            Page::Signing => "key-round",
            Page::Terminal => "square-terminal",
            Page::Identities => "user-round",
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
