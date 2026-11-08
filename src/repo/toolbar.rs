//! The tab's toolbar: undo / redo, remote and repository buttons, the current branch, and the
//! status of the last operation.

use std::path::PathBuf;
use std::time::Duration;

use egui::Ui;

use super::pull::pull_menu;
use super::{Action, Dialog, RepoTab, ci_status, widgets};
use crate::a11y::Named;
use crate::icons;
use crate::keymap::{self, Command};
use crate::menu_item::MenuItem;
use crate::settings::Settings;
use crate::theme::Tokens;

/// "Undo Commit (Ctrl+Z)", or "Undo Claude Code's Commit (Ctrl+Z)" for an agent's step.
fn step_tip(what: &str, entry: &super::timeline::Entry, keys: &str) -> String {
    match entry.undo.as_ref().and_then(|u| u.agent.as_ref()) {
        Some(agent) => crate::i18n::message(
            "repo-core-3645",
            &[
                ("what", format!("{}", what).into()),
                ("agent", format!("{}", agent).into()),
                (
                    "label",
                    crate::repo::messages::display_operation(&entry.label).into(),
                ),
                ("keys", format!("{}", keys).into()),
            ],
        ),
        None => crate::i18n::message(
            "repo-core-3646",
            &[
                ("what", format!("{}", what).into()),
                (
                    "label",
                    crate::repo::messages::display_operation(&entry.label).into(),
                ),
                ("keys", format!("{}", keys).into()),
            ],
        ),
    }
}

/// A robot in the corner of the undo button `undo`: the step it undoes is an agent's (its
/// tooltip names which).
fn agent_badge(ui: &Ui, undo: &egui::Response) {
    let t = Tokens::get(ui.ctx());
    let size = 11.0;
    let at = undo.rect.right_bottom() - egui::vec2(size * 0.75, size * 0.75);
    let rect = egui::Rect::from_center_size(at, egui::Vec2::splat(size + 3.0));
    ui.painter()
        .circle_filled(rect.center(), (size + 3.0) / 2.0, t.chrome);
    icons::paint(ui, rect.shrink(1.5), "bot", size, t.accent);
}

impl RepoTab {
    pub(super) fn toolbar(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let idle = self.busy_op.is_none();
            self.nav_buttons(ui, acts);
            ui.add_space(4.0);
            self.history_buttons(ui, idle, acts);
            widgets::toolbar_separator(ui);
            self.remote_buttons(ui, acts);
            widgets::toolbar_separator(ui);
            self.repo_buttons(ui, idle, acts);
            widgets::toolbar_separator(ui);
            if icons::tool_button(ui, "refresh-cw", "", !self.loading)
                .named(crate::i18n::text("repo-core-3647"))
                .on_hover_text(keymap::tip(
                    ui.ctx(),
                    crate::i18n::text("repo-core-0070"),
                    Command::Reload,
                ))
                .clicked()
            {
                acts.push(Action::Reload);
            }
            widgets::toolbar_separator(ui);
            ui.menu_button(crate::i18n::text("repo-core-3648"), |ui| {
                for (label, page) in [
                    (
                        crate::i18n::text("repo-core-0022"),
                        super::file_comparison::Page::CodeMoves,
                    ),
                    (
                        crate::i18n::text("repo-core-0024"),
                        super::file_comparison::Page::Migration,
                    ),
                    (
                        crate::i18n::text("repo-core-0026"),
                        super::file_comparison::Page::Lens,
                    ),
                    (
                        crate::i18n::text("repo-core-0028"),
                        super::file_comparison::Page::Matrix,
                    ),
                ] {
                    if ui.button(label).clicked() {
                        acts.push(Action::OpenFileComparison(page));
                        ui.close();
                    }
                }
            });
            self.head_label(ui, idle, acts);
            self.target_conflict_badge(ui);
            self.network_chip(ui, acts);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.fresh_rewrite_button(ui, acts);
                self.op_status(ui, acts);
                self.hint_status(ui);
                // What is left between the branch and the status moves the window.
                let rest = ui.available_rect_before_wrap();
                crate::app::title_bar::window_drag(ui, rest, "toolbar");
            });
        });
    }

    /// Undo and redo, with what they'd undo or redo as tooltips, and the undo tree.
    fn history_buttons(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let undo = self.timeline.current_entry();
        let undo_tip = match undo {
            Some(e) => step_tip(
                crate::i18n::text("repo-core-0051"),
                e,
                &keymap::label(ui.ctx(), keymap::Command::Undo),
            ),
            None => crate::i18n::text("repo-core-2862").to_owned(),
        };
        let waits = self.waits_text();
        let why = |nothing: bool, none: &str| match &waits {
            _ if nothing => none.to_owned(),
            Some(w) => w.clone(),
            None => none.to_owned(),
        };
        let undo = icons::tool_button(
            ui,
            "undo-2",
            "",
            idle && self.timeline.current_entry().is_some(),
        )
        .named(&undo_tip)
        .on_hover_text(&undo_tip)
        .on_disabled_hover_text(why(
            self.timeline.current_entry().is_none(),
            crate::i18n::text("repo-core-2862"),
        ));
        if undo.clicked() {
            acts.push(Action::Undo);
        }
        // What Ctrl+Z takes back next is an agent's.
        if self
            .timeline
            .current_entry()
            .and_then(|e| e.undo.as_ref())
            .is_some_and(|e| e.agent.is_some())
        {
            agent_badge(ui, &undo);
        }
        let redo = self.timeline.redo_target();
        let redo_tip = match redo {
            Some(e) => step_tip(
                crate::i18n::text("repo-core-0052"),
                e,
                &keymap::label(ui.ctx(), keymap::Command::Redo),
            ),
            None => crate::i18n::text("repo-core-2863").to_owned(),
        };
        if icons::tool_button(ui, "redo-2", "", idle && redo.is_some())
            .named(&redo_tip)
            .on_hover_text(redo_tip)
            .on_disabled_hover_text(why(
                self.timeline.redo_target().is_none(),
                crate::i18n::text("repo-core-2863"),
            ))
            .clicked()
        {
            acts.push(Action::Redo);
        }
        if icons::tool_button(ui, "history", "", true)
            .on_hover_text(crate::i18n::message(
                "repo-core-3651",
                &[(
                    "Timeline",
                    format!("{}", keymap::label(ui.ctx(), keymap::Command::Timeline)).into(),
                )],
            ))
            .clicked()
        {
            acts.push(Action::ToggleTimeline);
        }
    }

    /// Fetch, pull and push, with how far the current branch is behind and ahead.
    /// Fetch, pull and push stay available while another operation runs: they wait for it.
    fn remote_buttons(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        // Offline mode: off, saying why, rather than left to wait for a network.
        let offline = super::offline::refused_text();
        let has_remote = !self.remotes.is_empty() && offline.is_none();
        let off_why = offline.unwrap_or_else(|| crate::i18n::text("repo-core-2861").to_owned());
        let no_remote = crate::i18n::text("repo-core-2861");
        let waits = self
            .waits_text()
            .map(|_| {
                crate::i18n::message(
                    "repo-core-3652",
                    &[(
                        "self",
                        format!("{}", self.busy_op.as_deref().unwrap_or_default()).into(),
                    )],
                )
            })
            .unwrap_or_default();
        let fetched = crate::i18n::message(
            "repo-core-3653",
            &[
                (
                    "Fetch",
                    format!(
                        "{}",
                        keymap::tip(
                            ui.ctx(),
                            crate::i18n::text("repo-core-3654"),
                            Command::Fetch
                        )
                    )
                    .into(),
                ),
                (
                    "elapsed",
                    format!("{}", short_duration(self.last_fetch.elapsed())).into(),
                ),
                ("waits", format!("{}", waits).into()),
            ],
        );
        if icons::tool_button(
            ui,
            "cloud-download",
            crate::i18n::text("repo-core-0029"),
            has_remote,
        )
        .on_hover_text(fetched)
        .on_disabled_hover_ui(|ui| self.why_not(ui, Command::Fetch))
        .on_disabled_hover_text(no_remote)
        .on_disabled_hover_text(&off_why)
        .clicked()
        {
            acts.push(Action::Fetch);
        }
        let (ahead, behind) = self
            .refs
            .current
            .as_ref()
            .and_then(|c| self.refs.local.iter().find(|b| &b.name == c))
            .map(|b| b.ahead_behind())
            .unwrap_or((0, 0));
        let mode = Settings::get(ui.ctx()).pull_mode;
        let pull = icons::tool_button_count(
            ui,
            "arrow-down-to-line",
            crate::i18n::text("repo-core-0030"),
            behind,
            has_remote,
        )
        .named(|| behind_ahead(crate::i18n::text("repo-core-0030"), behind, "behind"))
        .on_hover_text(format!("{}{waits}", mode.tip(ui.ctx())))
        .on_disabled_hover_text(&off_why);
        crate::repo::ghost::offer(&pull, || crate::repo::ghost::GhostOp::Pull(mode));
        if pull.clicked() {
            acts.push(Action::Pull);
        }
        crate::menu_item::context(&pull)
            .style(crate::theme::menu_style)
            .show(|ui| pull_menu(ui, acts));
        let more = ui.add_enabled_ui(has_remote, |ui| {
            icons::small_button(ui, "chevron-down", crate::i18n::text("repo-core-3658"))
        });
        egui::Popup::menu(&more.inner)
            .style(crate::theme::menu_style)
            .show(|ui| pull_menu(ui, acts));
        if self.gerrit.is_some() {
            let push = icons::tool_button(
                ui,
                "arrow-up-from-line",
                crate::i18n::text("repo-core-0031"),
                has_remote && !self.refs.head.is_empty(),
            )
            .on_hover_text(crate::i18n::text("repo-core-3659"));
            if push.clicked()
                && let Some(dialog) = self.review_dialog()
            {
                acts.push(Action::Dialog(dialog));
            }
            super::hooks::push_menu(&push, acts);
            return;
        }
        let push = icons::tool_button_count(
            ui,
            "arrow-up-from-line",
            crate::i18n::text("repo-core-0032"),
            ahead,
            has_remote,
        )
        .named(|| behind_ahead(crate::i18n::text("repo-core-0032"), ahead, "ahead"))
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3661"),
            Command::Push,
        ))
        .on_disabled_hover_text(no_remote);
        if push.clicked() {
            acts.push(Action::Push);
        }
        super::hooks::push_menu(&push, acts);
        let sync_tip = crate::i18n::message(
            "repo-core-3662",
            &[("mode", format!("{}", mode.args().join(" ")).into())],
        );
        if icons::tool_button(
            ui,
            "arrow-down-up",
            crate::i18n::text("repo-core-3664"),
            has_remote,
        )
        .on_hover_text(sync_tip)
        .on_disabled_hover_text(&off_why)
        .clicked()
        {
            acts.push(Action::Sync);
        }
    }

    /// Why the greyed-out button of `command` can't be used now: what its key would say.
    fn why_not(&self, ui: &mut Ui, command: Command) {
        if let Err(why) = self.key_action(command) {
            ui.label(why);
        }
    }

    /// Branch, stash, terminal, search and the folder menu.
    fn repo_buttons(&mut self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let head = self.refs.head.clone();
        let waits = self.waits_text();
        if icons::tool_button(
            ui,
            "git-branch-plus",
            crate::i18n::text("repo-core-3665"),
            idle && !head.is_empty(),
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3666"),
            Command::NewBranch,
        ))
        .on_disabled_hover_ui(|ui| self.why_not(ui, Command::NewBranch))
        .on_disabled_hover_text(match &waits {
            _ if head.is_empty() => crate::i18n::text("repo-core-3667").to_owned(),
            Some(w) => w.clone(),
            None => String::new(),
        })
        .clicked()
        {
            acts.push(Action::Dialog(Dialog::create_branch(&head, "HEAD")));
        }
        if icons::tool_button(
            ui,
            "archive",
            crate::i18n::text("repo-core-0862"),
            idle && !self.work.is_clean(),
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3668"),
            Command::Stash,
        ))
        .on_disabled_hover_text(match &waits {
            _ if self.work.is_clean() => crate::i18n::text("repo-core-2867").to_owned(),
            Some(w) => w.clone(),
            None => String::new(),
        })
        .clicked()
        {
            acts.push(Action::Dialog(Dialog::stash()));
        }
        let terminal = icons::tool_button(
            ui,
            "square-terminal",
            crate::i18n::text("repo-core-3669"),
            true,
        )
        .on_hover_text(keymap::tip(
            ui.ctx(),
            crate::i18n::text("repo-core-3670"),
            Command::Terminal,
        ));
        if self.terminal_open {
            widgets::active_underline(ui, terminal.rect, t.accent);
        }
        if terminal.clicked() {
            acts.push(Action::ToggleTerminal);
        }
        self.branch_bands_header(ui);
        let activity = icons::tool_button(ui, "list", "", true)
            .on_hover_text(crate::i18n::text("repo-core-3671"));
        if self.activity.open {
            widgets::active_underline(ui, activity.rect, t.accent);
        }
        if activity.clicked() {
            acts.push(Action::ToggleActivity);
        }
        let search = icons::tool_button(ui, "search", "", true)
            .named(crate::i18n::text("repo-core-0057"))
            .on_hover_text(crate::i18n::message(
                "repo-core-3672",
                &[(
                    "Search",
                    format!("{}", keymap::label(ui.ctx(), keymap::Command::Search)).into(),
                )],
            ));
        if self.search.open {
            widgets::active_underline(ui, search.rect, t.accent);
        }
        if search.clicked() {
            acts.push(Action::ToggleSearch);
        }
        if ui
            .button(crate::i18n::text("repo-core-3673"))
            .on_hover_text(crate::i18n::text("repo-core-3674"))
            .clicked()
        {
            acts.push(Action::OpenTour);
        }
        let doctor = icons::tool_button(ui, "wrench", "", true)
            .on_hover_text(crate::i18n::text("repo-core-3675"));
        if self.doctor.is_some() {
            widgets::active_underline(ui, doctor.rect, t.accent);
        }
        if doctor.clicked() {
            acts.push(if self.doctor.is_some() {
                Action::CloseDoctor
            } else {
                Action::OpenDoctor(super::doctor::Section::ALL[0])
            });
        }
        let open = icons::tool_button(ui, "external-link", "", true)
            .named(crate::i18n::text("repo-core-3676"))
            .on_hover_text(crate::i18n::text("repo-core-3677"));
        let page = self.web_page();
        egui::Popup::menu(&open)
            .style(crate::theme::menu_style)
            .show(|ui| {
                if ui.button(crate::i18n::text("repo-core-3678")).clicked() {
                    acts.push(Action::OpenSshSetup);
                    ui.close();
                }
                if MenuItem::new(crate::i18n::text("repo-core-3679"))
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::ReviewTools(super::review_workflow::ToolsAct::Open));
                    ui.close();
                }
                let browser = MenuItem::new(crate::i18n::text("repo-core-0115"))
                    .icon("external-link")
                    .refused(
                        page.is_none()
                            .then_some(crate::i18n::text("repo-core-3680")),
                    )
                    .show(ui);
                if let Some(url) = &page {
                    let browser = browser.on_hover_text(url);
                    if browser.clicked() {
                        acts.push(Action::OpenUrl(url.clone()));
                    }
                }
                // Over SSH, nothing of this machine can open the folder.
                let here = crate::remote::openable_here(&self.path);
                let why = here.as_ref().err().map(String::as_str);
                if MenuItem::new(reveal_label())
                    .icon("folder-open")
                    .refused(why)
                    .explained_below(true)
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenPath(PathBuf::new()));
                }
                if MenuItem::new(crate::i18n::text("repo-core-0113"))
                    .icon("code")
                    .refused(why)
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenInEditor(PathBuf::new()));
                }
                if ui.button(crate::i18n::text("repo-core-3525")).clicked() {
                    // Elsewhere, the path there: what a shell on that machine takes.
                    let path = crate::remote::parse(&self.path)
                        .map_or_else(|| self.path.display().to_string(), |t| t.path);
                    ui.ctx().copy_text(path);
                }
            });
    }

    /// The checked-out branch (or detached HEAD), and a merge or rebase in progress.
    fn head_label(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        // A repository on another machine says which, first.
        if let Some(target) = crate::remote::parse(&self.path) {
            widgets::chip(ui, &target.host.label(), t.accent_fg).on_hover_text(
                crate::i18n::message(
                    "repo-core-3681",
                    &[
                        ("label", format!("{}", target.host.label()).into()),
                        ("path", format!("{}", target.path).into()),
                    ],
                ),
            );
            ui.add_space(6.0);
        }
        match &self.refs.current {
            Some(branch) => {
                icons::inline(ui, "git-branch", 14.0, t.text_dim);
                let label = ui.label(egui::RichText::new(branch).font(crate::theme::medium(12.5)));
                if let Some(e) = &self.ci.error {
                    label.on_hover_text(crate::i18n::message(
                        "repo-core-3682",
                        &[("e", format!("{}", e).into())],
                    ));
                }
            }
            None if self.refs.head.is_empty() => {
                ui.label(
                    egui::RichText::new(crate::i18n::text("repo-core-3683")).color(t.text_dim),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new(crate::i18n::message(
                        "repo-core-3684",
                        &[(
                            "head",
                            format!("{}", self.refs.head.get(..8).unwrap_or(&self.refs.head))
                                .into(),
                        )],
                    ))
                    .color(t.warning),
                );
            }
        }
        if let Some(status) = self.ci_status(&self.refs.head) {
            ui.add_space(4.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            let id = ui.id().with("head-ci");
            acts.extend(ci_status::dot(
                ui,
                id,
                rect.center(),
                &self.refs.head,
                status,
            ));
        }

        if let Some(state) = self.work.in_progress {
            ui.add_space(10.0);
            widgets::chip(
                ui,
                &crate::i18n::message(
                    "repo-core-0818",
                    &[("label", format!("{}", state.label()).into())],
                ),
                t.warning,
            );
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::message(
                        "repo-core-3686",
                        &[("fold", format!("{}", state.label().to_lowercase()).into())],
                    )),
                )
                .clicked()
            {
                acts.push(self.abort_for(state));
            }
        }
    }

    /// While an operation runs, what it is doing and _Cancel_; else the last operation's
    /// outcome (right to left).
    fn op_status(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        // While one runs, only a problem (one that couldn't wait for it) is worth showing.
        if let Some((msg, is_err)) = &self.status
            && (self.busy_op.is_none() || *is_err)
        {
            let color = if *is_err { t.danger } else { t.text_dim };
            let first_line = super::messages::display_status(&headline(msg));
            let entry = self.status_entry();
            let mut label =
                egui::Label::new(egui::RichText::new(first_line).color(color)).truncate();
            if entry.is_some() {
                label = label.sense(egui::Sense::click());
            }
            let shown = ui.add(label).on_hover_ui(|ui| {
                ui.label(super::messages::display_status(msg));
                if entry.is_some() {
                    ui.label(
                        egui::RichText::new(crate::i18n::text("repo-core-3687")).color(t.text_dim),
                    );
                }
            });
            if let Some(id) = entry
                && shown.clicked()
            {
                acts.push(Action::ShowActivity(id));
            }
        }
        let Some(op) = &self.busy_op else {
            if self.loading {
                ui.add(crate::spinner::Spinner::new());
            }
            return;
        };
        if self.can_cancel()
            && ui
                .button(crate::i18n::text("repo-core-0443"))
                .on_hover_text(if self.waiting.is_empty() {
                    crate::i18n::message(
                        "repo-core-3688",
                        &[("op", crate::repo::messages::display_operation(&op).into())],
                    )
                } else {
                    crate::i18n::message(
                        "repo-core-3689",
                        &[("op", crate::repo::messages::display_operation(&op).into())],
                    )
                })
                .clicked()
        {
            acts.push(Action::CancelOp);
        }
        let op = super::messages::display_operation(op);
        let progress = self.op_progress();
        let doing = match &progress {
            _ if self.was_cancelled() => crate::i18n::message(
                "repo-core-3690",
                &[("op", crate::repo::messages::display_operation(&op).into())],
            ),
            Some(p) => format!("{op}… {}", p.label()),
            None => format!("{op}…"),
        };
        let label = ui.label(egui::RichText::new(doing).color(t.text_dim));
        if let Some(p) = &progress
            && !p.detail.is_empty()
        {
            label.on_hover_text(format!("{}: {}", p.phase, p.detail));
        }
        ui.add(crate::spinner::Spinner::new());
        if !self.waiting.is_empty() {
            let names: Vec<String> = self
                .waiting
                .iter()
                .map(|w| super::messages::display_operation(w.label()))
                .collect();
            widgets::chip(
                ui,
                &crate::i18n::message(
                    "repo-core-3693",
                    &[("names", format!("{}", names.join(", ")).into())],
                ),
                t.text_dim,
            )
            .on_hover_text(crate::i18n::message(
                "repo-core-3694",
                &[("op", crate::repo::messages::display_operation(&op).into())],
            ));
        }
    }

    /// Why a button that changes the repository waits: the operation running.
    fn waits_text(&self) -> Option<String> {
        self.busy_op.as_ref().map(|op| {
            crate::i18n::message(
                "repo-core-3695",
                &[("op", crate::repo::messages::display_operation(&op).into())],
            )
        })
    }
}

pub(crate) fn reveal_label() -> &'static str {
    if cfg!(windows) {
        crate::i18n::text("repo-core-3696")
    } else if cfg!(target_os = "macos") {
        crate::i18n::text("repo-core-3698")
    } else {
        crate::i18n::text("repo-core-3699")
    }
}

/// Pull or Push as a screen reader says it: "Pull, 2 behind".
fn behind_ahead(name: &str, n: u32, how: &str) -> String {
    if n == 0 {
        return name.to_owned();
    }
    crate::i18n::message(
        match how {
            "behind" => "repo-core-toolbar-behind",
            _ => "repo-core-toolbar-ahead",
        },
        &[("name", name.into()), ("count", n.into())],
    )
}

fn short_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => crate::i18n::message("repo-core-3701", &[("s", format!("{}", s).into())]),
        60..3600 => crate::i18n::message("repo-core-2943", &[("s", format!("{}", s / 60).into())]),
        _ => crate::i18n::message("repo-core-2944", &[("s", format!("{}", s / 3600).into())]),
    }
}

/// The line of a git error worth showing in the toolbar: conflicts and `error:`/`fatal:` lines
/// beat progress chatter like "Auto-merging x".
pub(super) fn headline(msg: &str) -> String {
    let (label, body) = msg.split_once(": ").unwrap_or(("", msg));
    let conflicts = body.lines().filter(|l| l.starts_with("CONFLICT")).count();
    if conflicts > 0 {
        let files = if conflicts == 1 { "file" } else { "files" };
        let them = |n: usize| if n == 1 { "it" } else { "them" };
        return match super::conflict::rerere::reused_count(body) {
            0 => crate::i18n::message(
                "repo-core-3702",
                &[
                    (
                        "label",
                        crate::repo::messages::display_operation(&label).into(),
                    ),
                    ("conflicts", (conflicts).into()),
                ],
            ),
            n if n >= conflicts => crate::i18n::message(
                "repo-core-3703",
                &[
                    (
                        "label",
                        crate::repo::messages::display_operation(&label).into(),
                    ),
                    ("n", (n).into()),
                ],
            ),
            n => crate::i18n::message(
                "repo-core-3704",
                &[
                    (
                        "label",
                        crate::repo::messages::display_operation(&label).into(),
                    ),
                    ("conflicts", (conflicts).into()),
                    ("n", (n).into()),
                ],
            ),
        };
    }
    // A hook named as what stopped it (see `git::FailedHook`) says more than git's own
    // `error: failed to push some refs` after it.
    let hook = body.lines().next().filter(|l| {
        (l.starts_with("the ") || l.starts_with("it was done, but the ")) && l.contains(" hook ")
    });
    let line = hook
        .or_else(|| {
            body.lines()
                .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
        })
        .or_else(|| body.lines().find(|l| !l.trim().is_empty()))
        .unwrap_or_default();
    if label.is_empty() {
        line.to_owned()
    } else {
        format!("{label}: {line}")
    }
}
