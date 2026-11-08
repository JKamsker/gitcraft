//! Drawing the views of a repository tab that are popped out, the buttons that pop them out
//! and bring them back, and what the tab's actions do to them.

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
            Panel::Terminal => self.terminal_open,
            Panel::Pr => self.pr_view.is_some(),
            Panel::Ci => self.ci_view.is_some(),
        }
    }

    /// What the window of `panel` shows, for its title.
    fn popout_subject(&self, panel: Panel) -> String {
        match panel {
            Panel::Diff => match &self.diff {
                Some(view) => format!("Diff of {}", view.key.path()),
                None => "Diff".to_owned(),
            },
            Panel::Terminal => match self.terminals.get(self.active_terminal) {
                Some(term) if self.terminal_open => format!("Terminal ({})", term.shell),
                _ => "Terminal".to_owned(),
            },
            Panel::Pr => match &self.pr_view {
                Some(view) => match &view.details {
                    Some(Ok(details)) => {
                        format!("Pull request #{}: {}", view.number, details.title)
                    }
                    _ => format!("Pull request #{}", view.number),
                },
                None => "Pull request".to_owned(),
            },
            Panel::Ci => {
                // As the CI view's header says it.
                let noun = match self.forge() {
                    Some(Forge::GitLab) => "Pipelines",
                    _ => "Workflow runs",
                };
                match &self.ci_view {
                    Some(view) => format!("{noun} of {}", crate::repo::short(&view.sha)),
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
        self.popout_status(ui, panel);
        if !self.view_open(panel) {
            placeholder(ui, panel, absent(panel), acts);
        } else if panel == Panel::Diff && self.conflict.is_some() {
            let why = "The conflict editor is open in the main window.";
            placeholder(ui, panel, why, acts);
        } else {
            match panel {
                Panel::Diff => self.diff_view(ui, acts),
                Panel::Terminal => self.terminal_panel(ui, acts),
                Panel::Pr => self.pr_view_ui(ui, acts),
                Panel::Ci => self.ci_view_ui(ui, acts),
            }
        }
        // Inside the main window, its keys are the main window's (see `RepoTab::shortcuts`).
        if class != ViewportClass::EmbeddedWindow {
            self.popout_keys(ui.ctx(), panel, acts);
        }
    }

    /// The tab's latest message (an operation that failed, done from this window or not) at
    /// the foot of the window, where the toolbar showing it may be out of sight.
    fn popout_status(&self, ui: &mut Ui, panel: Panel) {
        let Some((text, failed)) = &self.status else {
            return;
        };
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
    fn popout_keys(&self, ctx: &egui::Context, panel: Panel, acts: &mut Vec<Action>) {
        if panel == Panel::Terminal || self.dialog.is_some() || ctx.egui_wants_keyboard_input() {
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
            allowed.extend([
                Command::NextChange,
                Command::PreviousChange,
                Command::NextChangeAlt,
                Command::PreviousChangeAlt,
            ]);
        }
        keymap::owner(ctx, egui::Id::new((&self.path, panel)));
        keymap::active(ctx, scopes);
        keymap::restrict(ctx, Some(allowed));
        let window_commands = keymap::pressed(ctx, Scope::Window);
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
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
                if !self.terminal_open {
                    self.toggle_terminal(ctx);
                }
                self.popouts.pop_out(Panel::Terminal);
            }
            Request::PopOut(panel) => {
                if self.view_open(panel) {
                    self.popouts.pop_out(panel);
                } else {
                    self.status = Some((format!("There's no {} open", panel.noun()), true));
                }
            }
            Request::Dock(panel) => {
                self.popouts.dock(panel);
            }
            Request::DockAll => {
                if !self.popouts.dock_all() {
                    self.status = Some(("No view is in a window of its own".to_owned(), true));
                }
            }
            // Its pop-out went when the action was noted (see `popout_intercept`).
            Request::Close(panel) => match panel {
                Panel::Diff => self.apply(Action::CloseDiff, ctx),
                Panel::Pr => self.apply(Action::ClosePr, ctx),
                Panel::Ci => self.apply(Action::CloseCi, ctx),
                Panel::Terminal => self.terminal_open = false,
            },
            Request::Focus(panel) => self.focus_popout(panel, ctx),
            Request::PopShown => match self.shown_poppable() {
                Ok(panel) => {
                    self.popouts.pop_out(panel);
                }
                Err(why) => self.status = Some((why.to_owned(), true)),
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
            Action::ToggleTerminal if self.terminal_open && !self.docked(Panel::Terminal) => {
                self.focus_popout(Panel::Terminal, ctx);
                None
            }
            Action::Select(_)
            | Action::SelectHash(_)
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
            return Err("Nothing to pop out: finish or close the conflict editor first");
        }
        if self.rebase.is_some() {
            return Err("Nothing to pop out: finish or close the interactive rebase first");
        }
        let nothing = if self.popouts.any() {
            "Nothing else to pop out: open a diff, a pull request or the CI view first"
        } else {
            "Nothing to pop out: open a diff, a pull request or the CI view first"
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
        Panel::Diff => "No diff open. Click a file in the main window to show its diff here.",
        Panel::Terminal => "The terminal is hidden; its shells keep running.",
        Panel::Pr => "No pull request open. Open one in the main window to show it here.",
        Panel::Ci => "No CI view open. Open the builds of a commit to show them here.",
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
            if panel == Panel::Terminal && ui.button("Show the terminal").clicked() {
                acts.push(Action::ToggleTerminal);
            }
            if ui.button("Close this window").clicked() {
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
            format!("Bring the {} back into the main window", panel.noun()),
            Request::Dock(panel),
        )
    } else {
        (
            "picture-in-picture-2",
            format!("Pop the {} out into its own window", panel.noun()),
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
    if popped { "Close (Esc)" } else { docked_tip }
}

#[cfg(test)]
mod registry_tests {
    use super::*;
    use crate::keymap::{self, Command, Scope, Stroke};

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
