//! The tab's toolbar: undo / redo, remote and repository buttons, the current branch, and the
//! status of the last operation.

use crate::repo::terminals::TerminalAct;
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
        Some(agent) => format!("{what} {agent}'s {} ({keys})", entry.label),
        None => format!("{what} {} ({keys})", entry.label),
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
        ui.horizontal_wrapped(|ui| {
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
                .named("Reload (F5)")
                .on_hover_text(keymap::tip("Reload", Command::Reload))
                .clicked()
            {
                acts.push(Action::Reload);
            }
            widgets::toolbar_separator(ui);
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
            Some(e) => step_tip("Undo", e, &keymap::written("Cmd+Z")),
            None => "Nothing to undo".to_owned(),
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
            "Nothing to undo",
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
            Some(e) => step_tip("Redo", e, &keymap::written("Cmd+Y")),
            None => "Nothing to redo".to_owned(),
        };
        if icons::tool_button(ui, "redo-2", "", idle && redo.is_some())
            .named(&redo_tip)
            .on_hover_text(redo_tip)
            .on_disabled_hover_text(why(
                self.timeline.redo_target().is_none(),
                "Nothing to redo",
            ))
            .clicked()
        {
            acts.push(Action::Redo);
        }
        if icons::tool_button(ui, "history", "", true)
            .on_hover_text("Timeline and undo tree (Ctrl+Shift+Z)")
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
        let off_why = offline.unwrap_or_else(|| "This repository has no remote".to_owned());
        let no_remote = "This repository has no remote";
        let waits = self
            .waits_text()
            .map(|_| {
                format!(
                    "\nClick now and it starts after {}",
                    self.busy_op.as_deref().unwrap_or_default()
                )
            })
            .unwrap_or_default();
        let fetched = format!(
            "{}\nLast fetched {} ago{waits}",
            keymap::tip(
                "Fetch all remotes (git fetch --all --prune)",
                Command::Fetch
            ),
            short_duration(self.last_fetch.elapsed())
        );
        if icons::tool_button(ui, "cloud-download", "Fetch", has_remote)
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
        let pull = icons::tool_button_count(ui, "arrow-down-to-line", "Pull", behind, has_remote)
            .named(|| behind_ahead("Pull", behind, "behind"))
            .on_hover_text(format!("{}{waits}", mode.tip()))
            .on_disabled_hover_text(&off_why);
        crate::repo::ghost::offer(&pull, || crate::repo::ghost::GhostOp::Pull(mode));
        if pull.clicked() {
            acts.push(Action::Pull);
        }
        egui::Popup::context_menu(&pull)
            .style(crate::theme::menu_style)
            .show(|ui| pull_menu(ui, acts));
        let more = ui.add_enabled_ui(has_remote, |ui| {
            icons::small_button(ui, "chevron-down", "What Pull does, and Fetch all")
        });
        egui::Popup::menu(&more.inner)
            .style(crate::theme::menu_style)
            .show(|ui| pull_menu(ui, acts));
        if self.gerrit.is_some() {
            let push = icons::tool_button(
                ui,
                "arrow-up-from-line",
                "Push for review",
                has_remote && !self.refs.head.is_empty(),
            )
            .on_hover_text(
                "Submit commits to Gerrit; amend and submit again to update the patch set",
            );
            if push.clicked()
                && let Some(dialog) = self.review_dialog()
            {
                acts.push(Action::Dialog(dialog));
            }
            super::hooks::push_menu(&push, acts);
            return;
        }
        let push = icons::tool_button_count(ui, "arrow-up-from-line", "Push", ahead, has_remote)
            .named(|| behind_ahead("Push", ahead, "ahead"))
            .on_hover_text(keymap::tip(
                "Push the current branch (publishes it if it has no upstream yet)",
                Command::Push,
            ))
            .on_disabled_hover_text(no_remote);
        if push.clicked() {
            acts.push(Action::Push);
        }
        super::hooks::push_menu(&push, acts);
        let sync_tip = format!(
            "Pull, then push the current branch (git {}, then git push)\nIf the pull fails \
             (conflicts, or it can't fast-forward), nothing is pushed",
            mode.args().join(" ")
        );
        if icons::tool_button(ui, "arrow-down-up", "Sync", has_remote)
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
    fn repo_buttons(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let head = self.refs.head.clone();
        let waits = self.waits_text();
        if icons::tool_button(ui, "git-branch-plus", "Branch", idle && !head.is_empty())
            .on_hover_text(keymap::tip("Create a branch at HEAD", Command::NewBranch))
            .on_disabled_hover_ui(|ui| self.why_not(ui, Command::NewBranch))
            .on_disabled_hover_text(match &waits {
                _ if head.is_empty() => "No commit to branch from yet".to_owned(),
                Some(w) => w.clone(),
                None => String::new(),
            })
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::create_branch(&head, "HEAD")));
        }
        if icons::tool_button(ui, "archive", "Stash", idle && !self.work.is_clean())
            .on_hover_text(keymap::tip(
                "Stash everything, staged changes, a snapshot, or picked files",
                Command::Stash,
            ))
            .on_disabled_hover_text(match &waits {
                _ if self.work.is_clean() => "No changes to stash".to_owned(),
                Some(w) => w.clone(),
                None => String::new(),
            })
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::stash()));
        }
        let terminal = icons::tool_button(ui, "square-terminal", "Terminal", true).on_hover_text(
            keymap::tip("Terminal in the repository folder", Command::Terminal),
        );
        if self.terminals.is_open() {
            widgets::active_underline(ui, terminal.rect, t.accent);
        }
        if terminal.clicked() {
            acts.push(Action::Terminal(TerminalAct::Toggle));
        }
        let activity = icons::tool_button(ui, "list", "", true)
            .on_hover_text("Activity: each operation's git commands and all they printed");
        if self.activity.open {
            widgets::active_underline(ui, activity.rect, t.accent);
        }
        if activity.clicked() {
            acts.push(Action::ToggleActivity);
        }
        let search = icons::tool_button(ui, "search", "", true)
            .named("Search commits")
            .on_hover_text(format!("Search commits ({})", keymap::written("Cmd+F")));
        if self.search.open {
            widgets::active_underline(ui, search.rect, t.accent);
        }
        if search.clicked() {
            acts.push(Action::ToggleSearch);
        }
        let doctor = icons::tool_button(ui, "wrench", "", true).on_hover_text(
            "Repository doctor: ignore rules, hooks, health, configuration, hidden changes",
        );
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
            .named("Open the folder or the repository's page")
            .on_hover_text("Open the folder or the repository's page…");
        egui::Popup::menu(&open)
            .style(crate::theme::menu_style)
            .show(|ui| {
                crate::menu_item::bounded_menu(ui, "open-in", |ui| self.open_in_items(ui, acts))
            });
    }

    fn open_in_items(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let page = self.web_page();
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
        if MenuItem::new("Open in editor")
            .icon("code")
            .refused(why)
            .show(ui)
            .clicked()
        {
            acts.push(Action::OpenInEditor(PathBuf::new()));
        }
        ui.add_space(4.0);
        if MenuItem::new("Copy path").icon("copy").show(ui).clicked() {
            // Elsewhere, the path there: what a shell on that machine takes.
            let path = crate::remote::parse(&self.path)
                .map_or_else(|| self.path.display().to_string(), |t| t.path);
            ui.ctx().copy_text(path);
        }
        ui.separator();
        let browser = MenuItem::new("Open in browser")
            .icon("external-link")
            .refused(page.is_none().then_some("No remote with a web page"))
            .show(ui);
        if let Some(url) = &page {
            let browser = browser.on_hover_text(url);
            if browser.clicked() {
                acts.push(Action::OpenUrl(url.clone()));
            }
        }
    }

    /// The checked-out branch (or detached HEAD), and a merge or rebase in progress.
    fn head_label(&self, ui: &mut Ui, idle: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        // A repository on another machine says which, first.
        if let Some(target) = crate::remote::parse(&self.path) {
            widgets::chip(ui, &target.host.label(), t.accent_fg).on_hover_text(format!(
                "git runs on {}, in {}",
                target.host.label(),
                target.path
            ));
            ui.add_space(6.0);
        }
        match &self.refs.current {
            Some(branch) => {
                icons::inline(ui, "git-branch", 14.0, t.text_dim);
                let label = ui.label(egui::RichText::new(branch).font(crate::theme::medium(12.5)));
                if let Some(e) = &self.ci.error {
                    label.on_hover_text(format!("No build status: {e}"));
                }
            }
            None if self.refs.head.is_empty() => {
                ui.label(egui::RichText::new("No commits yet").color(t.text_dim));
            }
            None => {
                ui.label(
                    egui::RichText::new(format!(
                        "Detached HEAD at {}",
                        self.refs.head.get(..8).unwrap_or(&self.refs.head)
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
            widgets::chip(ui, &format!("{} in progress", state.label()), t.warning);
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(format!("Abort {}", state.label().to_lowercase())),
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
            let first_line = headline(msg);
            let entry = self.status_entry();
            let mut label =
                egui::Label::new(egui::RichText::new(first_line).color(color)).truncate();
            if entry.is_some() {
                label = label.sense(egui::Sense::click());
            }
            let shown = ui.add(label).on_hover_ui(|ui| {
                ui.label(msg.as_str());
                if entry.is_some() {
                    ui.label(
                        egui::RichText::new(
                            "Click for its git commands and all they printed (Activity)",
                        )
                        .color(t.text_dim),
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
                .button("Cancel")
                .on_hover_text(if self.waiting.is_empty() {
                    format!("Stop {op}, and what it started")
                } else {
                    format!("Stop {op}, and what it started; what waits for it won't run")
                })
                .clicked()
        {
            acts.push(Action::CancelOp);
        }
        let progress = self.op_progress();
        let doing = match &progress {
            _ if self.was_cancelled() => format!("Cancelling {op}…"),
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
            let names: Vec<&str> = self.waiting.iter().map(|w| w.label()).collect();
            widgets::chip(ui, &format!("then {}", names.join(", ")), t.text_dim).on_hover_text(
                format!("Start after {op}, in this order. Cancel or a failure drops them."),
            );
        }
    }

    /// Why a button that changes the repository waits: the operation running.
    fn waits_text(&self) -> Option<String> {
        self.busy_op
            .as_ref()
            .map(|op| format!("Available once {op} is done"))
    }
}

pub(crate) fn reveal_label() -> &'static str {
    if cfg!(windows) {
        "Show in Explorer"
    } else if cfg!(target_os = "macos") {
        "Show in Finder"
    } else {
        "Open folder"
    }
}

/// Pull or Push as a screen reader says it: "Pull, 2 behind".
fn behind_ahead(name: &str, n: u32, how: &str) -> String {
    let count = (n > 0).then(|| format!("{n} {how}"));
    crate::a11y::labels::join([name, count.as_deref().unwrap_or("")])
}

fn short_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min", s / 60),
        _ => format!("{} h", s / 3600),
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
            0 => format!("{label}: conflicts in {conflicts} {files}, resolve them in Changes"),
            n if n >= conflicts => format!(
                "{label}: {n} {} resolved as before, check {} in Changes",
                if n == 1 { "file" } else { "files" },
                them(n)
            ),
            n => format!(
                "{label}: conflicts in {conflicts} {files}, {n} resolved as before: check {}, \
                 resolve the rest in Changes",
                them(n)
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

#[cfg(test)]
mod area_d_tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn repository_toolbar_keeps_trailing_actions_reachable_at_narrow_scales() {
        for width in [640.0, 800.0, 1400.0] {
            for scale in [1.0, 1.25, 1.5] {
                let ctx = egui::Context::default();
                let mut tab = RepoTab::new_deferred("/nonexistent/toolbar-layout".into(), &ctx);
                tab.refs.current = Some("feature/a-long-branch-name-with-many-words".into());
                let mut h = egui_kittest::Harness::builder()
                    .with_size([width / scale, 500.0 / scale])
                    .with_pixels_per_point(scale)
                    .build_ui_state(
                        |ui, (tab, fonts, actions): &mut (RepoTab, bool, Vec<Action>)| {
                            if !*fonts {
                                crate::theme::install_fonts(ui.ctx());
                                egui_extras::install_image_loaders(ui.ctx());
                                *fonts = true;
                                return;
                            }
                            egui::Panel::top("toolbar-test").show(ui, |ui| {
                                tab.toolbar(ui, actions);
                            });
                        },
                        (tab, false, Vec::new()),
                    );
                h.run();
                let reload = h.get_by_label("Reload").rect();
                assert!(
                    h.ctx.content_rect().contains_rect(reload),
                    "{width}/{scale}: {reload:?}"
                );
                h.get_by_label("Reload").click();
                h.run();
                assert!(h.state().2.iter().any(|a| matches!(a, Action::Reload)));
            }
        }
    }

    #[test]
    fn area_d_open_in_order_icons_remote_reasons_and_copy_scope() -> Result<(), String> {
        for width in [640.0, 1100.0, 1600.0] {
            for scale in [1.0, 1.25, 1.5] {
                for path in [
                    "/nonexistent/local repo",
                    "ssh://host/home/user/remote repo",
                ] {
                    let ctx = egui::Context::default();
                    let tab = RepoTab::new_deferred(path.into(), &ctx);
                    let mut h = egui_kittest::Harness::builder()
                        .with_size([width / scale, 760.0 / scale])
                        .with_pixels_per_point(scale)
                        .build_ui_state(
                            |ui, (tab, fonts): &mut (RepoTab, bool)| {
                                if !*fonts {
                                    crate::theme::install_fonts(ui.ctx());
                                    egui_extras::install_image_loaders(ui.ctx());
                                    *fonts = true;
                                    return;
                                }
                                let open = ui.button("Open In");
                                egui::Popup::menu(&open)
                                    .style(crate::theme::menu_style)
                                    .show(|ui| {
                                        crate::menu_item::bounded_menu(ui, "open-in", |ui| {
                                            tab.open_in_items(ui, &mut Vec::new())
                                        });
                                    });
                            },
                            (tab, false),
                        );
                    h.run();
                    h.get_by_label("Open In").click();
                    h.run();
                    let manager = h.get_by_label(reveal_label()).rect();
                    let editor = h.get_by_label("Open in editor").rect();
                    let copy = h.get_by_label("Copy path").rect();
                    let web = h.get_by_label("Open in browser").rect();
                    assert!(
                        manager.bottom() <= editor.top()
                            && editor.bottom() <= copy.top()
                            && copy.bottom() < web.top()
                    );
                    h.get_by_label("No remote with a web page");
                    if path.starts_with("ssh:") {
                        assert!(
                            h.query_all_by_label_contains("programs of this machine")
                                .next()
                                .is_some()
                        );
                    } else if let Ok(dir) = std::env::var("GITCRAFT_AREA_D_SCREENSHOTS") {
                        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                        h.render()?
                            .save(format!("{dir}/open-in-{width:.0}-{scale:.2}.png"))
                            .map_err(|e| e.to_string())?;
                    }
                    h.get_by_label("Copy path").click();
                    h.step();
                    let expected = crate::remote::parse(std::path::Path::new(path))
                        .map_or_else(|| path.to_owned(), |remote| remote.path);
                    assert!(h.output().platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == &expected)));
                    h.run();
                }
            }
        }
        Ok(())
    }
}
