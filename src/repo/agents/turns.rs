//! Agent turns in _Uncommitted changes_: the turns `gitcraft checkpoint` saved in this
//! worktree, each labelled with its prompt; a click diffs the turn against the one before it.

use egui::{RichText, Ui};

use super::AgentAct;
use crate::agent::checkpoint::{self, Turn};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// At most this many characters of a prompt name a turn.
const LABEL_CHARS: usize = 48;

impl RepoTab {
    /// Reads the turns again when the history or the working tree was read again.
    pub(super) fn reload_turns(&mut self, ctx: &egui::Context) {
        let key = (self.history_gen, self.work_read);
        if self.agents.turns_reading || self.history_gen == 0 || self.agents.turns_for == Some(key)
        {
            return;
        }
        self.agents.turns_reading = true;
        self.agents.turns_for = Some(key);
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let top = crate::git::run(Some(&repo), &["rev-parse", "--show-toplevel"])?;
                checkpoint::turns(&repo, top.trim_end_matches(['\n', '\r']))
            },
            |tab, result: Result<Vec<Turn>, String>, _| {
                tab.agents.turns_reading = false;
                // No turns to show when they can't be read: nothing to say about it either.
                tab.agents.turns = result.unwrap_or_default();
            },
        );
    }

    pub(super) fn install_hooks(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.status = Some(("Installing the agent checkpoint hooks…".to_owned(), false));
        self.then(
            ctx,
            move || crate::agent::hooks::install(&repo),
            |tab, result, _| {
                tab.status = Some(match result {
                    Ok(said) => (format!("Agent checkpoints on. {}", said.join(". ")), false),
                    Err(e) => (
                        format!("The agent checkpoint hooks weren't installed: {e}"),
                        true,
                    ),
                });
            },
        );
    }

    pub(super) fn forget_turns(&mut self, session: String, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.then(
            ctx,
            move || checkpoint::forget(&repo, &session),
            |tab, result, _| {
                tab.agents.turns_for = None;
                if let Err(e) = result {
                    tab.status = Some((format!("The turns weren't deleted: {e}"), true));
                }
            },
        );
    }

    /// The strip of turns at the top of the working tree's changes, if there are any.
    pub(in crate::repo) fn agent_turns(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let turns = &self.agents.turns;
        if turns.is_empty() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("AGENT TURNS")
                        .font(theme::semibold(10.5))
                        .color(t.text_faint),
                )
                .on_hover_text(
                    "Saved by the agent's hook after each turn. Click one to see what that turn \
                     changed; right-click for everything since it",
                );
                let mut session: Option<&str> = None;
                for (i, turn) in turns.iter().enumerate() {
                    if session != Some(turn.session.as_str()) {
                        session = Some(&turn.session);
                        session_line(ui, turn, turns, &t, acts);
                    }
                    turn_chip(ui, turn, turns.get(i.wrapping_sub(1)), &t, acts);
                }
            });
        ui.separator();
    }
}

fn session_line(ui: &mut Ui, turn: &Turn, turns: &[Turn], t: &Tokens, acts: &mut Vec<Action>) {
    let count = turns.iter().filter(|x| x.session == turn.session).count();
    let r = ui.label(
        RichText::new(format!(
            "Session {} · {count} turn{}",
            short_session(&turn.session),
            if count == 1 { "" } else { "s" }
        ))
        .small()
        .color(t.text_dim),
    );
    let session = turn.session.clone();
    r.context_menu(|ui| {
        if ui
            .button("Delete this session's turns")
            .on_hover_text("The saved turns go; the files stay as they are")
            .clicked()
        {
            acts.push(Action::Agent(AgentAct::ForgetTurns(session.clone())));
        }
    });
}

fn turn_chip(ui: &mut Ui, turn: &Turn, before: Option<&Turn>, t: &Tokens, acts: &mut Vec<Action>) {
    let label = shorten(&turn.label);
    let text = format!("{} · {label}", turn.number);
    let r = ui
        .add(
            egui::Button::selectable(false, RichText::new(text).size(12.0).color(t.text))
                .truncate(),
        )
        .on_hover_text(hover(turn));
    let before_label = match before.filter(|b| b.session == turn.session) {
        Some(b) => format!("turn {}", b.number),
        None => "before turn 1".to_owned(),
    };
    if r.clicked() {
        acts.push(Action::Compare {
            base: turn.parent.clone(),
            base_label: before_label,
            target: Some(turn.commit.clone()),
            target_label: format!("turn {}: {label}", turn.number),
            since_fork: false,
        });
    }
    r.context_menu(|ui| {
        if ui.button("What this turn changed").clicked() {
            acts.push(Action::Compare {
                base: turn.parent.clone(),
                base_label: format!("before turn {}", turn.number),
                target: Some(turn.commit.clone()),
                target_label: format!("turn {}", turn.number),
                since_fork: false,
            });
        }
        if ui
            .button("Everything since this turn")
            .on_hover_text("This turn against the files as they are now")
            .clicked()
        {
            acts.push(Action::Compare {
                base: turn.commit.clone(),
                base_label: format!("turn {}", turn.number),
                target: None,
                target_label: "now".to_owned(),
                since_fork: false,
            });
        }
    });
}

fn hover(turn: &Turn) -> String {
    let prompt = if turn.prompt.is_empty() {
        "(no prompt recorded)"
    } else {
        turn.prompt.as_str()
    };
    let when = crate::git::relative_time(turn.time, crate::git::now());
    format!(
        "Turn {} · {when}\n\n{prompt}\n\nClick: what this turn changed",
        turn.number
    )
}

fn shorten(label: &str) -> String {
    if label.chars().count() > LABEL_CHARS {
        format!(
            "{}…",
            label.chars().take(LABEL_CHARS - 1).collect::<String>()
        )
    } else {
        label.to_owned()
    }
}

fn short_session(session: &str) -> &str {
    session.get(..8).unwrap_or(session)
}

#[cfg(test)]
mod tests {
    use super::{short_session, shorten};

    #[test]
    fn long_prompts_and_sessions_are_shortened() {
        assert_eq!(shorten("short"), "short");
        let long = "ü".repeat(60);
        assert_eq!(shorten(&long).chars().count(), 48);
        assert_eq!(short_session("0123456789"), "01234567");
        assert_eq!(short_session("äb"), "äb");
    }
}
