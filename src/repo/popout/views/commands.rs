//! Keyboard dispatch, pop/dock requests, focus, and action interception.

use super::*;

impl RepoTab {
    /// Esc closes the view in its window as in the main one; in a diff, F7 / Shift+F7 (or
    /// Alt+↓ / Alt+↑) go to the next and previous change. Ctrl+Z / Ctrl+Y undo and redo, F5
    /// reloads. The terminal keeps every key.
    pub(in crate::repo::popout::views) fn popout_keys(
        &mut self,
        ctx: &egui::Context,
        panel: Panel,
        acts: &mut Vec<Action>,
    ) {
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
    pub(in crate::repo::popout::views) fn focus_popout(&self, panel: Panel, ctx: &egui::Context) {
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
    pub(in crate::repo::popout::views) fn shown_poppable(&self) -> Result<Panel, &'static str> {
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
