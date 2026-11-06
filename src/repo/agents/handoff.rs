//! _Hand it to the agent_: notes written on the lines of any diff (as the review of a pull
//! request writes them, but kept here, never published), collected in a tray, and typed into
//! the worktree's agent terminal as one message; and the same for a conflict, a failing CI
//! run's log and a pull request's thread. The messages are made in [`crate::agent::handoff`].

use std::collections::HashMap;

use egui::{RichText, Stroke, Ui};

use super::{AgentAct, AgentsState};
use crate::agent::handoff::{self, Note};
use crate::agent::tasks::{KNOWN, agent_label};
use crate::pull_request::DiffSide;
use crate::repo::diff::{DiffData, Kind};
use crate::repo::pr_review::{DiffReview, LineTarget};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// The notes of this worktree and the one being written.
#[derive(Default)]
pub(crate) struct Tray {
    pub(super) notes: Vec<Note>,
    loaded: bool,
    pub(in crate::repo) target: Option<LineTarget>,
    pub(super) text: String,
    focus: bool,
    /// The terminal picked to send to, by its place.
    pub(super) send_to: Option<usize>,
    /// Counts the changes, so that saves landing out of order keep the latest.
    changes: u64,
}

impl RepoTab {
    /// The notes of the file `path` as a review of its diff: marks on the lines that have one,
    /// the line being written on.
    pub(in crate::repo) fn note_review(&self, path: &str) -> DiffReview {
        let tray = &self.agents.tray;
        let mut marks: HashMap<(DiffSide, u64), usize> = HashMap::new();
        for note in tray.notes.iter().filter(|n| n.path == path) {
            let side = if note.old {
                DiffSide::Left
            } else {
                DiffSide::Right
            };
            *marks.entry((side, note.line)).or_insert(0) += 1;
        }
        DiffReview {
            marks,
            target: tray.target.clone().filter(|t| t.path == path),
            path: path.to_owned(),
            local: true,
        }
    }

    /// Reads the tray kept for this worktree, once.
    pub(super) fn load_tray(&mut self, ctx: &egui::Context) {
        if std::mem::replace(&mut self.agents.tray.loaded, true) {
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            move || handoff::load(&repo),
            |tab, result, _| match result {
                Ok(mut notes) => {
                    // Written meanwhile: kept after those read.
                    notes.append(&mut tab.agents.tray.notes);
                    tab.agents.tray.notes = notes;
                }
                Err(e) => tab.status = Some((e, true)),
            },
        );
    }

    fn save_tray(&mut self, ctx: &egui::Context) {
        self.agents.tray.changes += 1;
        let seq = self.agents.tray.changes;
        let repo = self.path.clone();
        let notes = self.agents.tray.notes.clone();
        self.then(
            ctx,
            move || handoff::save_in_order(&repo, &notes, seq),
            |tab, result, _| {
                if let Err(e) = result {
                    tab.status = Some((e, true));
                }
            },
        );
    }

    pub(super) fn tray_act(&mut self, act: TrayAct, ctx: &egui::Context) {
        let tray = &mut self.agents.tray;
        match act {
            TrayAct::Open(target) => {
                tray.target = Some(target);
                tray.focus = true;
            }
            TrayAct::Close => {
                tray.target = None;
                tray.text.clear();
            }
            TrayAct::Add(code) => {
                let Some(target) = tray.target.take() else {
                    return;
                };
                tray.notes.push(Note {
                    path: target.path,
                    line: target.line,
                    old: target.side == DiffSide::Left,
                    code,
                    text: std::mem::take(&mut tray.text).trim().to_owned(),
                });
                self.save_tray(ctx);
            }
            TrayAct::Remove(i) => {
                if i < tray.notes.len() {
                    tray.notes.remove(i);
                    self.save_tray(ctx);
                }
            }
            TrayAct::Clear => {
                tray.notes.clear();
                self.save_tray(ctx);
            }
            TrayAct::SendTo(i) => tray.send_to = Some(i),
            TrayAct::SendNotes => {
                let message = handoff::notes_message(&self.agents.tray.notes);
                if self.send_to_agent(&message, "the notes") {
                    self.agents.tray.notes.clear();
                    self.save_tray(ctx);
                }
            }
        }
    }

    /// Types `message` into the agent's terminal of this tab: whether it went.
    pub(super) fn send_to_agent(&mut self, message: &str, what: &str) -> bool {
        let labels: Vec<&str> = self.terminals.iter().map(|t| t.shell.as_str()).collect();
        let agents: Vec<String> = KNOWN
            .iter()
            .map(|a| a.label.to_owned())
            .chain(self.agents.tasks.iter().map(|t| agent_label(&t.agent)))
            .collect();
        let picked = self
            .agents
            .tray
            .send_to
            .filter(|i| *i < labels.len())
            .or_else(|| handoff::pick_terminal(&labels, &agents, self.active_terminal));
        let Some(i) = picked else {
            self.status = Some((
                format!(
                    "No terminal to send {what} to: start the agent (Start agent task…, or its \
                     command in a terminal of this worktree) first"
                ),
                true,
            ));
            return false;
        };
        let Some(term) = self.terminals.get_mut(i) else {
            return false;
        };
        let label = term.shell.clone();
        match term.paste_message(message) {
            Ok(()) => {
                self.terminal_open = true;
                self.active_terminal = i;
                self.status = Some((
                    format!("Typed {what} into {label}'s terminal: press Enter there to send them"),
                    false,
                ));
                true
            }
            Err(e) => {
                self.status = Some((format!("{what} weren't sent to {label}: {e}"), true));
                false
            }
        }
    }

    /// The tray under the big area, while it has notes.
    pub(in crate::repo) fn tray_panel(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let tray = &self.agents.tray;
        if tray.notes.is_empty() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        egui::Panel::bottom("handoff_tray")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(t.card)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(Stroke::new(1.0, t.separator)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("NOTES FOR THE AGENT · {}", tray.notes.len()))
                            .font(theme::semibold(10.5))
                            .color(t.text_faint),
                    )
                    .on_hover_text("Only here: they go to the agent's terminal, nowhere else");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::repo::primary_button(ui, "Send to agent", !self.terminals.is_empty(), 110.0)
                            .on_hover_text("Types them into the agent's terminal as one message; then they're cleared")
                            .on_disabled_hover_text("Start the agent in a terminal of this worktree first")
                            .clicked()
                        {
                            acts.push(tray_act(TrayAct::SendNotes));
                        }
                        self.terminal_choice(ui, acts);
                        if ui.button("Clear").clicked() {
                            acts.push(tray_act(TrayAct::Clear));
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("handoff-notes")
                    .max_height(110.0)
                    .show(ui, |ui| {
                        for (i, note) in tray.notes.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if crate::icons::small_button(ui, "x", "Take this note out").clicked() {
                                    acts.push(tray_act(TrayAct::Remove(i)));
                                }
                                ui.label(
                                    RichText::new(format!("{}:{}", note.path, note.line))
                                        .font(theme::mono(11.5))
                                        .color(t.text_dim),
                                );
                                ui.add(egui::Label::new(first_line(&note.text)).truncate());
                            });
                        }
                    });
            });
    }

    fn terminal_choice(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if self.terminals.len() < 2 {
            return;
        }
        let labels: Vec<&str> = self.terminals.iter().map(|t| t.shell.as_str()).collect();
        let shown = self
            .agents
            .tray
            .send_to
            .and_then(|i| labels.get(i).copied())
            .unwrap_or("the agent's terminal");
        egui::ComboBox::from_id_salt("handoff-terminal")
            .selected_text(format!("to {shown}"))
            .show_ui(ui, |ui| {
                for (i, label) in labels.iter().enumerate() {
                    if ui
                        .selectable_label(self.agents.tray.send_to == Some(i), *label)
                        .clicked()
                    {
                        acts.push(tray_act(TrayAct::SendTo(i)));
                    }
                }
            });
    }
}

/// What the tray is asked for.
pub(crate) enum TrayAct {
    Open(LineTarget),
    Close,
    /// Adds the note being written (the line's code given).
    Add(String),
    Remove(usize),
    Clear,
    SendTo(usize),
    SendNotes,
}

#[cfg(test)]
impl Tray {
    /// As if the tray had been read: nothing kept for the worktree.
    pub(super) fn loaded_for_tests(&mut self) {
        self.loaded = true;
    }
}

fn tray_act(act: TrayAct) -> Action {
    Action::Agent(AgentAct::Tray(act))
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    if text.lines().count() > 1 {
        format!("{line} …")
    } else {
        line.to_owned()
    }
}

/// The panel under a diff to write a note on the line picked.
pub(in crate::repo) fn note_panel(
    ui: &mut Ui,
    t: &Tokens,
    agents: &mut AgentsState,
    data: &DiffData,
    acts: &mut Vec<Action>,
) {
    let tray = &mut agents.tray;
    let Some(target) = tray.target.clone() else {
        return;
    };
    let code = data
        .lines
        .iter()
        .find(|l| match target.side {
            DiffSide::Left => l.kind != Kind::Add && l.old.map(u64::from) == Some(target.line),
            DiffSide::Right => l.kind != Kind::Del && l.new.map(u64::from) == Some(target.line),
        })
        .map(|l| l.text.clone())
        .unwrap_or_default();
    egui::Panel::bottom("agent_note")
        .frame(
            egui::Frame::new()
                .fill(t.card)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .stroke(Stroke::new(1.0, t.separator)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let which = if target.side == DiffSide::Left {
                    " (as it was)"
                } else {
                    ""
                };
                ui.label(
                    RichText::new(format!(
                        "Note for the agent · {}:{}{which}",
                        target.path, target.line
                    ))
                    .font(theme::mono(11.5))
                    .color(t.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::small_button(ui, "x", "Close (Esc)").clicked() {
                        acts.push(tray_act(TrayAct::Close));
                    }
                });
            });
            let edit = ui.add(
                egui::TextEdit::multiline(&mut tray.text)
                    .hint_text("What the agent should change here")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut tray.focus) {
                edit.request_focus();
            }
            let submit = edit.has_focus()
                && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
            let valid = !tray.text.trim().is_empty();
            ui.horizontal(|ui| {
                if crate::repo::primary_button(ui, "Add to the tray", valid, 120.0)
                    .on_hover_text(format!("{}+Enter", crate::manual::command()))
                    .on_disabled_hover_text("Write the note first")
                    .clicked()
                    || submit && valid
                {
                    acts.push(tray_act(TrayAct::Add(code.clone())));
                }
                ui.label(
                    RichText::new("Kept here only, until you send the tray to the agent")
                        .small()
                        .color(t.text_faint),
                );
            });
        });
}

#[cfg(test)]
mod tests {
    use super::first_line;

    #[test]
    fn a_long_note_shows_its_first_line() {
        assert_eq!(first_line("one\ntwo"), "one …");
        assert_eq!(first_line("one"), "one");
        assert_eq!(first_line(""), "");
    }
}

#[cfg(test)]
#[path = "handoff_tests.rs"]
mod handoff_tests;
