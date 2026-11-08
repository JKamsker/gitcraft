//! Drawing the views of a repository tab that are popped out, the buttons that pop them out
//! and bring them back, and what the tab's actions do to them.

use crate::repo::terminals::TerminalAct;
use egui::{Key, RichText, Ui, ViewportClass};

use super::{Panel, Request, title, viewport_id, window};
use crate::forge::Forge;
use crate::icons;
use crate::repo::{Action, RepoTab, pr_review};
use crate::theme::Tokens;

impl RepoTab {
    #[cfg(test)]
    pub(crate) fn set_test_popout(&mut self, panel: Panel) {
        self.popouts.pop_out(panel);
    }

    /// Immutable declarations retained while a native nested event loop prevents
    /// the coordinator from borrowing this session to draw its immediate panels.
    pub(crate) fn popout_window_specs(&self) -> Vec<(egui::ViewportId, egui::ViewportBuilder)> {
        self.popouts
            .popped()
            .map(|panel| {
                (
                    viewport_id(&self.path, panel),
                    window::builder(&title(&self.name, &self.popout_subject(panel))),
                )
            })
            .collect()
    }

    /// Draws the windows of the views popped out of this tab, and carries out what is done in
    /// them. Called every frame for every repository tab after the tab shown is drawn (`shown`:
    /// this is that one), so switching tabs leaves the windows open; a tab closed stops calling
    /// it, which closes its windows.
    pub fn popouts_ui(&mut self, ctx: &egui::Context, shown: bool) {
        if !self.popouts.any() {
            return;
        }
        let mut acts = Vec::new();
        if shown {
            self.popped_hint(ctx, &mut acts);
        }
        let mut draw = self.popouts.take_drawn();
        if draw.len() < self.popouts.popped().count() {
            // Popped out this frame: its window opens on the next.
            ctx.request_repaint();
        }
        // Inside the main window, another tab's would float over the page shown.
        if ctx.embed_viewports() && !shown {
            draw.clear();
        }
        for panel in draw {
            let id = viewport_id(&self.path, panel);
            let title = title(&self.name, &self.popout_subject(panel));
            let scope = egui::Id::new(("popout", &self.path, panel));
            let closed = window::show(ctx, id, &title, |ui, class| {
                ui.push_id(scope, |ui| self.popped_view(ui, panel, class, &mut acts));
            });
            if closed {
                // Drawn in the main window from the next frame, which this one already drew.
                self.popouts.dock(panel);
                ctx.request_repaint();
            }
        }
        if !acts.is_empty() {
            ctx.request_repaint();
        }
        for action in acts {
            self.apply(action, ctx);
        }
    }

    /// Whether `panel` is drawn in the main window (it isn't popped out).
    pub(in crate::repo) fn docked(&self, panel: Panel) -> bool {
        !self.popouts.is_popped(panel)
    }

    /// Whether the main window's keys (Esc, F7) act on `panel`: it is drawn there, or its window
    /// is inside the main one, whose keys are the only ones.
    pub(in crate::repo) fn main_keys(&self, ctx: &egui::Context, panel: Panel) -> bool {
        self.docked(panel) || ctx.embed_viewports()
    }

    /// Whether the view of `panel` is open, popped out or not.
    fn view_open(&self, panel: Panel) -> bool {
        match panel {
            Panel::Diff => self.diff.is_some(),
            Panel::Terminal => self.terminals.is_open(),
            Panel::Pr => self.pr_view.is_some(),
            Panel::Ci => self.ci_view.is_some(),
        }
    }

    /// What the window of `panel` shows, for its title.
    fn popout_subject(&self, panel: Panel) -> String {
        match panel {
            Panel::Diff => match &self.diff {
                Some(view) => crate::i18n::message(
                    "repo-core-4215",
                    &[("path", view.key.path().to_string().into())],
                ),
                None => crate::i18n::text("repo-core-2367").to_owned(),
            },
            Panel::Terminal => match self.terminals.active() {
                Some(term) if self.terminals.is_open() => crate::i18n::message(
                    "repo-core-4216",
                    &[("shell", term.shell.to_string().into())],
                ),
                _ => crate::i18n::text("repo-core-3669").to_owned(),
            },
            Panel::Pr => match &self.pr_view {
                Some(view) => match &view.details {
                    Some(Ok(details)) => crate::i18n::message(
                        "repo-core-4217",
                        &[
                            ("number", format!("{}", view.number).into()),
                            ("title", details.title.to_string().into()),
                        ],
                    ),
                    _ => crate::i18n::message(
                        "repo-core-4218",
                        &[("number", format!("{}", view.number).into())],
                    ),
                },
                None => crate::i18n::text("repo-core-4219").to_owned(),
            },
            Panel::Ci => {
                // As the CI view's header says it.
                let noun = match self.forge() {
                    Some(Forge::GitLab) => crate::i18n::text("repo-core-4220"),
                    _ => crate::i18n::text("repo-core-4221"),
                };
                match &self.ci_view {
                    Some(view) => crate::i18n::message(
                        "repo-core-4222",
                        &[
                            ("noun", noun.to_string().into()),
                            ("sha", crate::repo::short(&view.sha).to_string().into()),
                        ],
                    ),
                    None => noun.to_owned(),
                }
            }
        }
    }

    /// The view of `panel` in its window, under the tab's latest message, and the keys it
    /// takes there.
    fn popped_view(
        &mut self,
        ui: &mut Ui,
        panel: Panel,
        class: ViewportClass,
        acts: &mut Vec<Action>,
    ) {
        if class != ViewportClass::EmbeddedWindow {
            self.popout_keys(ui.ctx(), panel, acts);
        }
        self.popout_status(ui, panel);
        if !self.view_open(panel) {
            placeholder(ui, panel, absent(panel), acts);
        } else if panel == Panel::Diff && self.conflict.is_some() {
            let why = crate::i18n::text("repo-core-4223");
            placeholder(ui, panel, why, acts);
        } else {
            match panel {
                Panel::Diff => self.diff_view(ui, acts),
                Panel::Terminal => self.terminal_panel(ui, acts),
                Panel::Pr => self.pr_view_ui(ui, acts),
                Panel::Ci => self.ci_view_ui(ui, acts),
            }
        }
    }

    /// The tab's latest message (an operation that failed, done from this window or not) at
    /// the foot of the window, where the toolbar showing it may be out of sight.
    fn popout_status(&self, ui: &mut Ui, panel: Panel) {
        let Some(notice) = &self.status else {
            return;
        };
        let (text, failed) = notice.raw();
        let text = notice.projected().unwrap_or_else(|| text.clone());
        let t = Tokens::get(ui.ctx());
        egui::Panel::bottom(egui::Id::new(("popout-status", &self.path, panel)))
            .frame(
                egui::Frame::new()
                    .fill(t.chrome)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| {
                let color = if *failed { t.danger } else { t.text_dim };
                ui.add(egui::Label::new(RichText::new(text).size(12.0).color(color)).truncate());
            });
    }

    /// Esc closes the view in its window as in the main one; in a diff, F7 / Shift+F7 (or
    /// Alt+↓ / Alt+↑) go to the next and previous change. Ctrl+Z / Ctrl+Y undo and redo, F5
    /// reloads. The terminal keeps every key.
    fn popout_keys(&mut self, ctx: &egui::Context, panel: Panel, acts: &mut Vec<Action>) {
        if panel == Panel::Terminal
            || self.dialog.is_some()
            || (crate::a11y::keyboard_taken(ctx) && !self.find_shortcut_exception(ctx))
        {
            return;
        }
        use crate::keymap::{self, Command, Scope};
        let mut allowed = vec![
            Command::Undo,
            Command::Redo,
            Command::Timeline,
            Command::Reload,
        ];
        let mut scopes = vec![Scope::Window, Scope::Repository];
        if panel == Panel::Diff && self.view_open(panel) {
            scopes.push(Scope::Changes);
            if self.find_in().is_some() {
                scopes.push(Scope::Selection);
                if self.find.open {
                    scopes.push(Scope::Find);
                }
                allowed.extend([
                    Command::Search,
                    Command::FindNext,
                    Command::FindPrevious,
                    Command::SelectionCopy,
                    Command::SelectionAll,
                    Command::SelectionHistory,
                ]);
            }
            allowed.extend([
                Command::NextChange,
                Command::PreviousChange,
                Command::NextChangeAlt,
                Command::PreviousChangeAlt,
            ]);
        }
        keymap::owner(ctx, egui::Id::new((&self.path, panel)));
        keymap::active(ctx, scopes);
        if self.find_shortcut_exception(ctx) {
            allowed.retain(|c| {
                matches!(
                    c,
                    Command::Search | Command::FindNext | Command::FindPrevious
                )
            });
        }
        keymap::restrict(ctx, Some(allowed));
        let window_commands = keymap::pressed(ctx, Scope::Window);
        if panel == Panel::Diff {
            self.find_keys(ctx, !crate::a11y::keyboard_taken(ctx));
        }
        let esc = !crate::a11y::keyboard_taken(ctx) && ctx.input(|i| i.key_pressed(Key::Escape));
        let undo = keymap::take(ctx, Command::Undo);
        let redo = keymap::take(ctx, Command::Redo);
        let reload = window_commands.contains(&Command::Reload);
        if keymap::take(ctx, Command::Timeline) {
            acts.push(Action::ToggleTimeline);
        }
        let open = self.view_open(panel);
        match panel {
            Panel::Diff if open => {
                if esc && self.line_comment_open() {
                    acts.push(Action::Pr(pr_review::PrAct::CloseLine));
                } else if esc {
                    acts.push(Action::CloseDiff);
                }
                for command in keymap::pressed(ctx, Scope::Changes) {
                    acts.push(Action::Key(command));
                }
            }
            Panel::Pr if esc && open => acts.push(Action::ClosePr),
            Panel::Ci if esc && open => acts.push(Action::CloseCi),
            _ => {}
        }
        if undo {
            acts.push(Action::Undo);
        }
        if redo {
            acts.push(Action::Redo);
        }
        if reload {
            acts.push(Action::Reload);
        }
        keymap::which_key(ctx, |command| self.command_refusal(command));
    }

    /// Carries out a pop-out button, the main window's hint or a palette entry.
    pub(in crate::repo) fn popout_request(&mut self, request: Request, ctx: &egui::Context) {
        match request {
            Request::PopOut(Panel::Terminal) => {
                if !self.terminals.is_open() {
                    self.terminals.toggle(&self.path, ctx);
                }
                self.popouts.pop_out(Panel::Terminal);
            }
            Request::PopOut(panel) => {
                if self.view_open(panel) {
                    self.popouts.pop_out(panel);
                } else {
                    self.status = Some(crate::repo::Notice::plain((
                        crate::i18n::message(
                            "repo-core-4225",
                            &[("noun", panel.noun().to_string().into())],
                        ),
                        true,
                    )));
                }
            }
            Request::Dock(panel) => {
                self.popouts.dock(panel);
            }
            Request::DockAll => {
                if !self.popouts.dock_all() {
                    self.status = Some(crate::repo::Notice::plain((
                        crate::i18n::text("repo-core-4226").to_owned(),
                        true,
                    )));
                }
            }
            // Its pop-out went when the action was noted (see `popout_intercept`).
            Request::Close(panel) => match panel {
                Panel::Diff => self.apply(Action::CloseDiff, ctx),
                Panel::Pr => self.apply(Action::ClosePr, ctx),
                Panel::Ci => self.apply(Action::CloseCi, ctx),
                Panel::Terminal => self.terminals.hide(),
            },
            Request::Focus(panel) => self.focus_popout(panel, ctx),
            Request::PopShown => match self.shown_poppable() {
                Ok(panel) => {
                    self.popouts.pop_out(panel);
                }
                Err(why) => self.status = Some(crate::repo::Notice::plain((why.to_owned(), true))),
            },
        }
        ctx.request_repaint();
    }

    /// Raises the window of `panel` above the others (inside the main window: its egui window).
    fn focus_popout(&self, panel: Panel, ctx: &egui::Context) {
        let id = viewport_id(&self.path, panel);
        if ctx.embed_viewports() {
            ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, egui::Id::new(id)));
        } else {
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
        }
    }

    /// What `action` does to the pop-outs, before the tab carries it out; `None` when that is
    /// all it does.
    ///
    /// A view closed on purpose takes its pop-out with it. The toolbar's Terminal raises a
    /// popped-out terminal rather than hiding it. Selecting a commit (or opening a pull request)
    /// leaves a popped-out pull request and CI view open, as they are on another screen.
    pub(in crate::repo) fn popout_intercept(
        &mut self,
        action: Action,
        ctx: &egui::Context,
    ) -> Option<Action> {
        self.popouts.note(&action, self.terminals.len());
        match action {
            Action::Terminal(TerminalAct::Toggle)
                if self.terminals.is_open() && !self.docked(Panel::Terminal) =>
            {
                self.focus_popout(Panel::Terminal, ctx);
                None
            }
            Action::Select(_)
            | Action::SelectHash(_)
            | Action::OpenCommitChanges(_)
            | Action::SelectPr(_)
            | Action::SelectStash(_)
            | Action::OpenPr(_) => {
                let keep_pr = !matches!(action, Action::OpenPr(_))
                    && !self.docked(Panel::Pr)
                    && self.pr_view.is_some();
                let keep_ci = !self.docked(Panel::Ci) && self.ci_view.is_some();
                if !keep_pr && !keep_ci {
                    return Some(action);
                }
                // Taken out of its way, then put back (not seen again here: they're gone).
                let pr = if keep_pr { self.pr_view.take() } else { None };
                let ci = if keep_ci { self.ci_view.take() } else { None };
                self.apply(action, ctx);
                if self.pr_view.is_none() {
                    self.pr_view = pr;
                }
                if self.ci_view.is_none() {
                    self.ci_view = ci;
                }
                None
            }
            other => Some(other),
        }
    }

    /// The view the big area shows that could pop out, as the big area picks it; why there is
    /// none otherwise.
    fn shown_poppable(&self) -> Result<Panel, &'static str> {
        if self.conflict.is_some() {
            return Err(crate::i18n::text("repo-core-4227"));
        }
        if self.rebase.is_some() {
            return Err(crate::i18n::text("repo-core-4228"));
        }
        let nothing = if self.popouts.any() {
            crate::i18n::text("repo-core-4229")
        } else {
            crate::i18n::text("repo-core-4230")
        };
        if self.diff.is_some() && self.docked(Panel::Diff) {
            return Ok(Panel::Diff);
        }
        if self.file_view.is_some() {
            return Err(nothing);
        }
        [Panel::Ci, Panel::Pr]
            .into_iter()
            .find(|p| self.view_open(*p) && self.docked(*p))
            .ok_or(nothing)
    }
}

/// What a pop-out window says while its view isn't open.
fn absent(panel: Panel) -> &'static str {
    match panel {
        Panel::Diff => crate::i18n::text("repo-core-4231"),
        Panel::Terminal => crate::i18n::text("repo-core-4232"),
        Panel::Pr => crate::i18n::text("repo-core-4233"),
        Panel::Ci => crate::i18n::text("repo-core-4234"),
    }
}

/// A window whose view isn't there: why, and the way out.
fn placeholder(ui: &mut Ui, panel: Panel, why: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 50.0).max(12.0));
        icons::inline(ui, "picture-in-picture-2", 32.0, t.text_faint);
        ui.add_space(10.0);
        ui.label(RichText::new(why).color(t.text_dim));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if panel == Panel::Terminal && ui.button(crate::i18n::text("repo-core-4235")).clicked()
            {
                acts.push(Action::Terminal(TerminalAct::Toggle));
            }
            if ui.button(crate::i18n::text("repo-core-4236")).clicked() {
                acts.push(Action::Popout(Request::Dock(panel)));
            }
        });
    });
}

/// The header button of `panel`'s view: Pop out in the main window, Bring back in its own.
pub(in crate::repo) fn button(ui: &mut Ui, panel: Panel, popped: bool, acts: &mut Vec<Action>) {
    let (icon, tip, request) = if popped {
        (
            "picture-in-picture",
            crate::i18n::message(
                "repo-core-4238",
                &[("noun", panel.noun().to_string().into())],
            ),
            Request::Dock(panel),
        )
    } else {
        (
            "picture-in-picture-2",
            crate::i18n::message(
                "repo-core-4239",
                &[("noun", panel.noun().to_string().into())],
            ),
            Request::PopOut(panel),
        )
    };
    let resp = icons::small_button(ui, icon, &tip);
    // Named for screen readers (and tests), as the icon says nothing to them.
    crate::a11y::a11y(&resp, egui::WidgetType::Button, &tip);
    if resp.clicked() {
        acts.push(Action::Popout(request));
    }
}

/// The close button's tooltip in a view's header: a popped-out view closes with its window.
pub(in crate::repo) fn close_tip(popped: bool, docked_tip: &'static str) -> &'static str {
    if popped {
        crate::i18n::text("repo-core-0711")
    } else {
        docked_tip
    }
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::keymap::{self, Command, Scope, Stroke};

    #[test]
    fn child_prepares_selection_context_and_find_field_keeps_escape() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recorded/panel-selection".into(), &ctx);
        tab.diff = Some(crate::repo::DiffView::new(crate::repo::DiffKey::Commit {
            hash: "recorded".into(),
            path: "file".into(),
        }));
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::SelectionCopy.id().into(),
            vec![Stroke::new(Key::F8, egui::Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::F8,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {
                let mut acts = Vec::new();
                tab.popout_keys(&ctx, Panel::Diff, &mut acts);
                assert!(
                    keymap::take(&ctx, Command::SelectionCopy),
                    "view gets its configured command after child context setup"
                );
                assert!(acts.is_empty());
            },
        )
        .drop_without_applying_deltas();
        tab.find.show();
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                // The production find field ID; a text primitive owns Escape before the diff.
                ui.add(
                    egui::TextEdit::singleline(&mut String::new())
                        .id(egui::Id::new("find_in_view_field")),
                )
                .request_focus();
                let mut acts = Vec::new();
                tab.popout_keys(&ctx, Panel::Diff, &mut acts);
                assert!(
                    acts.is_empty(),
                    "Escape in the find field must not close the entire diff"
                );
            },
        )
        .drop_without_applying_deltas();
    }

    #[test]
    fn actual_child_dispatch_uses_overrides_and_cancels_unrelated_commands() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recorded/panel-keys".into(), &ctx);
        tab.diff = Some(crate::repo::DiffView::new(crate::repo::DiffKey::Commit {
            hash: "recorded".into(),
            path: "file".into(),
        }));
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::Undo.id().into(),
            vec![Stroke::new(Key::F8, egui::Modifiers::NONE)],
        );
        settings.key_bindings.insert(
            Command::NextChange.id().into(),
            vec![Stroke::new(Key::F9, egui::Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        for (key, expected) in [
            (Key::F8, Some(Command::Undo)),
            (Key::F9, Some(Command::NextChange)),
            (Key::F5, Some(Command::Reload)),
            (Key::F7, None),
        ] {
            let mut acts = Vec::new();
            ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |_| tab.popout_keys(&ctx, Panel::Diff, &mut acts),
            )
            .drop_without_applying_deltas();
            match expected {
                Some(Command::Undo) => assert!(matches!(acts.as_slice(), [Action::Undo])),
                Some(Command::Reload) => assert!(matches!(acts.as_slice(), [Action::Reload])),
                Some(command) => {
                    assert!(matches!(acts.as_slice(), [Action::Key(found)] if *found == command))
                }
                None => assert!(acts.is_empty()),
            }
        }
        // A panel cannot consume a reconfigured window or remote-operation shortcut.
        assert!(!keymap::queued(&ctx, Command::Fetch));
        keymap::cancel(&ctx, Scope::Window);
    }
}
