//! The terminal panel owns its shells, selection, visibility and launch errors.
//! Collaborators add a completed launch or ask for a selection; they cannot bypass
//! the transitions that keep those values consistent. Hiding preserves the shells.

use std::path::{Path, PathBuf};

use egui::Ui;

use super::{Action, PaletteItem, RepoTab, widgets};
use crate::terminal::{ShellSpec, Terminal};
use crate::theme::Tokens;

/// One explicit route through the root dispatcher; local actions stay here.
pub(crate) enum TerminalAct {
    Toggle,
    New {
        shell: Option<ShellSpec>,
        cwd: Option<PathBuf>,
    },
    Close(usize),
    Select(usize),
}

/// `T` permits inert transition tests without spawning shells. Production always
/// uses `Terminal`, whose drop releases its owned PTY and child handle.
pub(super) struct Terminals<T = Terminal> {
    shells: Vec<T>,
    active: usize,
    open: bool,
    error: Option<String>,
}

impl<T> Default for Terminals<T> {
    fn default() -> Self {
        Self {
            shells: Vec::new(),
            active: 0,
            open: false,
            error: None,
        }
    }
}

impl<T> Terminals<T> {
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn len(&self) -> usize {
        self.shells.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.shells.is_empty()
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// A read-only view for labels and handoff targeting; collection mutation stays here.
    pub(super) fn shells(&self) -> &[T] {
        &self.shells
    }

    pub(super) fn active(&self) -> Option<&T> {
        self.shells.get(self.active)
    }

    /// Hides the panel without ending any shells, including a popped-out panel.
    pub(super) fn hide(&mut self) {
        self.open = false;
    }

    /// A launch from the panel, an agent or a custom command has the same transition.
    pub(super) fn opened(&mut self, result: Result<T, String>) {
        match result {
            Ok(shell) => {
                self.shells.push(shell);
                self.active = self.shells.len() - 1;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        self.open = true;
    }

    /// Keeps the selected shell when one before it closes; closes the panel with the last.
    pub(super) fn close(&mut self, index: usize) {
        if index < self.shells.len() {
            self.shells.remove(index);
            if index < self.active {
                self.active -= 1;
            }
        }
        self.active = self.active.min(self.shells.len().saturating_sub(1));
        if self.shells.is_empty() {
            self.hide();
        }
    }

    /// A stale UI selection never makes the active index invalid.
    fn select(&mut self, index: usize) {
        if index < self.shells.len() {
            self.active = index;
        }
    }

    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.shells.clear();
        self.active = 0;
        self.hide();
    }
}

impl Terminals {
    pub(super) fn apply(&mut self, action: TerminalAct, repo: &Path, ctx: &egui::Context) {
        match action {
            TerminalAct::Toggle => self.toggle(repo, ctx),
            TerminalAct::New { shell, cwd } => self.start(repo, shell, cwd, ctx),
            TerminalAct::Close(index) => self.close(index),
            TerminalAct::Select(index) => self.select(index),
        }
    }

    /// Shows or hides the panel, starting a shell the first time.
    pub(super) fn toggle(&mut self, repo: &Path, ctx: &egui::Context) {
        self.open = !self.open;
        if self.open && self.is_empty() {
            self.start(repo, None, None, ctx);
        }
    }

    fn start(
        &mut self,
        repo: &Path,
        shell: Option<ShellSpec>,
        cwd: Option<PathBuf>,
        ctx: &egui::Context,
    ) {
        let cwd = cwd.unwrap_or_else(|| repo.to_path_buf());
        // Remote shells run there, with a local temporary folder for the SSH/WSL client.
        let (shell, cwd) = match crate::remote::parse(&cwd) {
            Some(target) => {
                let (program, args, label) = crate::remote::terminal_command(&target);
                (
                    ShellSpec {
                        label,
                        program,
                        args,
                    },
                    std::env::temp_dir(),
                )
            }
            None => (
                shell.unwrap_or_else(|| crate::terminal::default_shell(ctx)),
                cwd,
            ),
        };
        self.opened(Terminal::spawn(&cwd, &shell, ctx));
    }

    /// A live shell keeps a hidden or parked worktree from being suspended.
    pub(super) fn has_running(&mut self) -> bool {
        self.shells.iter_mut().any(|term| !term.has_exited())
    }

    pub(super) fn handoff_targets(&mut self) -> Vec<(&str, bool)> {
        self.shells
            .iter_mut()
            .map(|term| {
                let exited = term.has_exited();
                (term.shell.as_str(), exited)
            })
            .collect()
    }

    /// Sends through the terminal's bracketed-paste safeguard and reveals it only on success.
    pub(super) fn paste_message(&mut self, index: usize, message: &str) -> Result<(), String> {
        let Some(term) = self.shells.get_mut(index) else {
            return Err("That terminal is no longer open".to_owned());
        };
        term.paste_message(message)?;
        self.select(index);
        self.open = true;
        Ok(())
    }

    pub(super) fn panel(&mut self, ui: &mut Ui, popped: bool, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        widgets::terminal_tabs(ui, &self.shells, self.active, popped, acts);
        if let Some(err) = self.error() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.colored_label(t.danger, err);
            });
            if self.is_empty() {
                return;
            }
        }
        let i = self.active;
        let Some(term) = self.shells.get_mut(i) else {
            return;
        };
        if term.has_exited() {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("The shell has exited.").color(t.text_dim));
                if ui.button("Close").clicked() {
                    acts.push(Action::Terminal(TerminalAct::Close(i)));
                }
            });
            return;
        }
        term.ui(ui);
    }
}

/// The tab supplies only the docking input; rendering needs no other feature's mutable state.
impl RepoTab {
    pub(super) fn terminal_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let popped = !self.docked(super::popout::Panel::Terminal);
        self.terminals.panel(ui, popped, acts);
    }
}

/// Named contribution points preserve the two palettes' existing order and presentation.
pub(super) fn toggle_palette_item() -> (String, String, Action) {
    (
        "Toggle terminal".into(),
        crate::keymap::label(crate::keymap::Command::Terminal),
        Action::Terminal(TerminalAct::Toggle),
    )
}

pub(super) fn new_palette_item() -> (String, String, Action) {
    (
        "New terminal".into(),
        "terminal".into(),
        Action::Terminal(TerminalAct::New {
            shell: None,
            cwd: None,
        }),
    )
}

pub(super) fn command_items() -> [PaletteItem; 2] {
    [
        PaletteItem::action(
            "Toggle terminal",
            "terminal",
            Action::Terminal(TerminalAct::Toggle),
        ),
        PaletteItem::action(
            "New terminal",
            "terminal",
            Action::Terminal(TerminalAct::New {
                shell: None,
                cwd: None,
            }),
        ),
    ]
}

#[cfg(test)]
mod tests;
