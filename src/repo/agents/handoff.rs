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
    /// The tray kept for the worktree has been read: before, a save would write over it.
    read: bool,
    /// Something changed before it was read: saved once it is.
    unsaved: bool,
    pub(in crate::repo) target: Option<LineTarget>,
    pub(super) text: String,
    focus: bool,
    /// The pass in which the note panel was last drawn (Esc closes it only while it shows).
    shown: u64,
    /// The terminal picked to send to: its place and its name then.
    pub(super) send_to: Option<(usize, String)>,
    /// Clear was clicked once: the next click empties the tray.
    confirm_clear: bool,
}

impl RepoTab {
    /// The notes of the file `path` written on the diff `source` as a review of that diff:
    /// marks on the lines that have one, the line being written on.
    pub(in crate::repo) fn note_review(&self, path: &str, source: &str) -> DiffReview {
        let tray = &self.agents.tray;
        let mut marks: HashMap<(DiffSide, u64), usize> = HashMap::new();
        for note in tray
            .notes
            .iter()
            .filter(|n| n.path == path && n.source == source)
        {
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

    /// Whether a note is being written under the diff shown (Esc closes it first).
    pub(in crate::repo) fn note_open(&self, ctx: &egui::Context) -> bool {
        self.agents.tray.target.is_some() && self.agents.tray.shown + 1 >= ctx.cumulative_pass_nr()
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
            |tab, result, ctx| {
                let tray = &mut tab.agents.tray;
                tray.read = true;
                match result {
                    Ok(mut notes) => {
                        // Written meanwhile: kept after those read.
                        notes.append(&mut tray.notes);
                        tray.notes = notes;
                    }
                    Err(e) => tab.status = Some((e, true)),
                }
                if std::mem::take(&mut tab.agents.tray.unsaved) {
                    tab.save_tray(ctx);
                }
            },
        );
    }

    fn save_tray(&mut self, ctx: &egui::Context) {
        if !self.agents.tray.read {
            self.agents.tray.unsaved = true;
            return;
        }
        let seq = handoff::next_change();
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
        if !matches!(act, TrayAct::Clear) {
            tray.confirm_clear = false;
        }
        match act {
            TrayAct::Open(target) => {
                tray.target = Some(target);
                tray.focus = true;
            }
            TrayAct::Close => {
                tray.target = None;
                tray.text.clear();
            }
            TrayAct::Add { code, source } => {
                let Some(target) = tray.target.take() else {
                    return;
                };
                tray.notes.push(Note {
                    path: target.path,
                    line: target.line,
                    old: target.side == DiffSide::Left,
                    code,
                    text: std::mem::take(&mut tray.text).trim().to_owned(),
                    source,
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
                let confirmed = tray.confirm_clear;
                tray.confirm_clear = !confirmed;
                if confirmed {
                    tray.notes.clear();
                    self.save_tray(ctx);
                }
            }
            TrayAct::SendTo(i) => {
                tray.send_to = self.terminals.get(i).map(|t| (i, t.shell.clone()));
            }
            TrayAct::SendNotes => {
                let message = handoff::notes_message(&self.agents.tray.notes);
                if self.send_to_agent(
                    &message,
                    crate::i18n::text("repo-workflows-agents-tray-act-78e896bb"),
                ) {
                    self.agents.tray.notes.clear();
                    self.save_tray(ctx);
                }
            }
        }
    }

    /// Which terminal a message would go to now (see [`handoff::pick_terminal`]).
    fn agent_terminal(&mut self) -> Option<usize> {
        let agents: Vec<String> = KNOWN
            .iter()
            .map(|a| a.label.to_owned())
            .chain(self.agents.tasks.iter().map(|t| agent_label(&t.agent)))
            .collect();
        let ended: Vec<bool> = self.terminals.iter_mut().map(|t| t.has_exited()).collect();
        let list: Vec<(&str, bool)> = self
            .terminals
            .iter()
            .map(|t| t.shell.as_str())
            .zip(ended)
            .collect();
        let picked = self
            .agents
            .tray
            .send_to
            .as_ref()
            .map(|(i, name)| (*i, name.as_str()));
        handoff::pick_terminal(&list, &agents, picked)
    }

    /// Types `message` (`what` it is) into the agent's terminal of this tab: whether it went.
    pub(super) fn send_to_agent(&mut self, message: &str, what: &str) -> bool {
        let Some(i) = self.agent_terminal() else {
            self.status = Some((
                format!(
                    "No agent terminal in this worktree to send {what} to: start the agent \
                     (Start agent task…), or pick its terminal next to Send to agent"
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
                    crate::i18n::message(
                        "repo-workflows-agent-send-confirmation",
                        &[
                            ("what", (what).into()),
                            ("terminal", (label.as_str()).into()),
                        ],
                    ),
                    false,
                ));
                true
            }
            Err(e) => {
                self.status = Some((
                    format!("Couldn't send {what} to the {label} terminal: {e}"),
                    true,
                ));
                false
            }
        }
    }

    /// The tray under the big area, while it has notes.
    pub(in crate::repo) fn tray_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let to = self.agent_terminal();
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
                        RichText::new(crate::i18n::message(
                            "repo-workflows-agents-tray-panel-1b67e032",
                            &[("tray-notes-len-1", (tray.notes.len()).clone().into())],
                        ))
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                    )
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-agents-tray-panel-86de1f26",
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::repo::primary_button(
                            ui,
                            crate::i18n::text("repo-workflows-agents-tray-panel-77a860cb"),
                            to.is_some(),
                            110.0,
                        )
                        .on_hover_text(crate::i18n::text(
                            "repo-workflows-agents-tray-panel-a2c648eb",
                        ))
                        .on_disabled_hover_text(crate::i18n::text(
                            "repo-workflows-agents-tray-panel-366ec196",
                        ))
                        .clicked()
                        {
                            acts.push(tray_act(TrayAct::SendNotes));
                        }
                        self.terminal_choice(ui, to, acts);
                        let clear = if tray.confirm_clear {
                            crate::i18n::message(
                                "repo-workflows-agents-tray-panel-ae4eaa14",
                                &[("tray-notes-len-1", (tray.notes.len()).clone().into())],
                            )
                        } else {
                            crate::i18n::text("repo-workflows-agents-tray-panel-83b12c22")
                                .to_owned()
                        };
                        if ui.button(clear).clicked() {
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
                                if crate::icons::small_button(
                                    ui,
                                    "x",
                                    crate::i18n::text("repo-workflows-agents-tray-panel-0451c67e"),
                                )
                                .clicked()
                                {
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

    /// Which terminal gets the notes: the one they would go to, and the others to pick.
    fn terminal_choice(&self, ui: &mut Ui, to: Option<usize>, acts: &mut Vec<Action>) {
        if self.terminals.is_empty() {
            return;
        }
        let labels: Vec<&str> = self.terminals.iter().map(|t| t.shell.as_str()).collect();
        let shown = to
            .and_then(|i| labels.get(i).copied())
            .unwrap_or(crate::i18n::text(
                "repo-workflows-agents-terminal-choice-f1f46570",
            ));
        egui::ComboBox::from_id_salt("handoff-terminal")
            .selected_text(crate::i18n::message(
                "repo-workflows-agents-terminal-choice-c0d8980e",
                &[("shown", (shown).clone().into())],
            ))
            .show_ui(ui, |ui| {
                for (i, label) in labels.iter().enumerate() {
                    if ui.selectable_label(to == Some(i), *label).clicked() {
                        acts.push(tray_act(TrayAct::SendTo(i)));
                    }
                }
            })
            .response
            .on_hover_text(crate::i18n::text(
                "repo-workflows-agents-terminal-choice-c22c5ccf",
            ));
    }
}

/// What the tray is asked for.
pub(crate) enum TrayAct {
    Open(LineTarget),
    Close,
    /// Adds the note being written: the line's code, and which diff it is on.
    Add {
        code: String,
        source: String,
    },
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
    source: &str,
    acts: &mut Vec<Action>,
) {
    let tray = &mut agents.tray;
    let Some(target) = tray.target.clone() else {
        return;
    };
    tray.shown = ui.ctx().cumulative_pass_nr();
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
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-agent-note-location",
                        &[
                            ("path", target.path.as_str().into()),
                            ("line", target.line.into()),
                            (
                                "old",
                                if target.side == DiffSide::Left {
                                    "yes"
                                } else {
                                    "no"
                                }
                                .into(),
                            ),
                        ],
                    ))
                    .font(theme::mono(11.5))
                    .color(t.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::small_button(
                        ui,
                        "x",
                        crate::i18n::text("repo-workflows-agents-note-panel-961b2a04"),
                    )
                    .clicked()
                    {
                        acts.push(tray_act(TrayAct::Close));
                    }
                });
            });
            let edit = ui.add(
                egui::TextEdit::multiline(&mut tray.text)
                    .hint_text(crate::i18n::text(
                        "repo-workflows-agents-note-panel-bb064d10",
                    ))
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
                if crate::repo::primary_button(
                    ui,
                    crate::i18n::text("repo-workflows-agents-note-panel-63565c04"),
                    valid,
                    120.0,
                )
                .on_hover_text(format!("{}+Enter", crate::manual::command()))
                .on_disabled_hover_text(crate::i18n::text(
                    "repo-workflows-agents-note-panel-4254e046",
                ))
                .clicked()
                    || submit && valid
                {
                    acts.push(tray_act(TrayAct::Add {
                        code: code.clone(),
                        source: source.to_owned(),
                    }));
                }
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-agents-note-panel-d065164e",
                    ))
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
