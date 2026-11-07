//! The bottom panel's terminals: opening, closing and drawing them.

use std::path::PathBuf;

use egui::Ui;

use super::{Action, RepoTab, widgets};
use crate::terminal::{ShellSpec, Terminal};
use crate::theme::Tokens;

impl RepoTab {
    /// Shows or hides the panel, starting a shell the first time.
    pub(super) fn toggle_terminal(&mut self, ctx: &egui::Context) {
        self.terminal_open = !self.terminal_open;
        if self.terminal_open && self.terminals.is_empty() {
            self.start_terminal(None, None, ctx);
        }
    }

    pub(super) fn start_terminal(
        &mut self,
        shell: Option<ShellSpec>,
        cwd: Option<PathBuf>,
        ctx: &egui::Context,
    ) {
        let cwd = cwd.unwrap_or_else(|| self.path.clone());
        // A folder on another machine: a shell there (the shell picked is one of this machine),
        // started from a folder of this one.
        let (shell, cwd) = match crate::remote::parse(&cwd) {
            Some(target) => {
                let (program, args, label) = crate::remote::terminal_command(&target);
                let shell = ShellSpec {
                    label,
                    program,
                    args,
                };
                (shell, std::env::temp_dir())
            }
            None => (
                shell.unwrap_or_else(|| crate::terminal::default_shell(ctx)),
                cwd,
            ),
        };
        let result = if self.practice.is_some() {
            Terminal::spawn_practice(&cwd, &shell, ctx)
        } else {
            Terminal::spawn(&cwd, &shell, ctx)
        };
        match result {
            Ok(term) => {
                self.terminals.push(term);
                self.active_terminal = self.terminals.len() - 1;
                self.terminal_error = None;
                self.terminal_open = true;
            }
            Err(e) => {
                self.terminal_error = Some(e);
                self.terminal_open = true;
            }
        }
    }

    /// Closes terminal `i`, and the panel with the last one.
    pub(super) fn close_terminal(&mut self, i: usize) {
        if i < self.terminals.len() {
            self.terminals.remove(i);
            // The one shown stays shown when one before it closes.
            if i < self.active_terminal {
                self.active_terminal -= 1;
            }
        }
        self.active_terminal = self
            .active_terminal
            .min(self.terminals.len().saturating_sub(1));
        if self.terminals.is_empty() {
            self.terminal_open = false;
        }
    }

    /// The bottom panel: terminal tabs and the active shell.
    pub(super) fn terminal_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        widgets::terminal_tabs(ui, &self.terminals, self.active_terminal, acts);
        if let Some(err) = &self.terminal_error {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.colored_label(t.danger, err);
            });
            if self.terminals.is_empty() {
                return;
            }
        }
        let i = self.active_terminal;
        let Some(term) = self.terminals.get_mut(i) else {
            return;
        };
        if term.has_exited() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("The shell has exited.").color(t.text_dim));
                if ui.button("Close").clicked() {
                    acts.push(Action::CloseTerminal(i));
                }
            });
            return;
        }
        term.ui(ui);
    }
}
