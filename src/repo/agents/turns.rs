//! Agent turns in _Uncommitted changes_: the turns `gitcraft checkpoint` saved in this
//! worktree, each labelled with its prompt; a click diffs the turn against the one before it.
//! The latest session shows; earlier ones fold away.

use egui::{RichText, Ui};

use super::AgentAct;
use crate::agent::checkpoint::{self, Turn};
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// At most this many characters of a prompt name a turn.
const LABEL_CHARS: usize = 48;
/// How tall a session's list of turns gets before it scrolls.
const LIST_H: f32 = 180.0;

impl RepoTab {
    /// Reads the turns again when the history was read again (a turn saved moves a ref).
    pub(super) fn reload_turns(&mut self, ctx: &egui::Context) {
        let key = self.history_gen;
        if self.agents.turns_reading || key == 0 || self.agents.turns_for == Some(key) {
            return;
        }
        self.agents.turns_reading = true;
        self.agents.turns_for = Some(key);
        let repo = self.path.clone();
        let known_top = self.agents.turns_top.clone();
        self.then(
            ctx,
            move || {
                let top = match known_top {
                    Some(top) => top,
                    None => crate::git::run(Some(&repo), &["rev-parse", "--show-toplevel"])?
                        .trim_end_matches(['\n', '\r'])
                        .to_owned(),
                };
                checkpoint::turns(&repo, &top).map(|turns| (top, turns))
            },
            |tab, result: Result<(String, Vec<Turn>), String>, _| {
                tab.agents.turns_reading = false;
                // No turns to show when they can't be read: nothing to say about it either.
                match result {
                    Ok((top, turns)) => {
                        tab.agents.turns_top = Some(top);
                        tab.agents.turns = turns;
                    }
                    Err(_) => tab.agents.turns.clear(),
                }
            },
        );
    }

    pub(super) fn install_hooks(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.status = Some((
            crate::i18n::text("repo-workflows-agents-install-hooks-2f5d6dc4").to_owned(),
            false,
        ));
        self.then(
            ctx,
            move || crate::agent::hooks::install(&repo),
            |tab, result, _| {
                tab.status = Some(match result {
                    Ok(said) => (said.join(" "), false),
                    Err(e) => (
                        crate::i18n::message(
                            "repo-workflows-agents-install-hooks-a8f2778c",
                            &[("e", (e).clone().into())],
                        ),
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
                    tab.status = Some((
                        crate::i18n::message(
                            "repo-workflows-agents-forget-turns-1d1d391e",
                            &[("e", (e).clone().into())],
                        ),
                        true,
                    ));
                }
            },
        );
    }

    /// The turns at the top of the working tree's changes, if there are any.
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
                    RichText::new(crate::i18n::text(
                        "repo-workflows-agents-agent-turns-94377c68",
                    ))
                    .font(theme::semibold(10.5))
                    .color(t.text_faint),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-agents-agent-turns-9a299007",
                ));
                for (n, session) in sessions(turns).into_iter().enumerate() {
                    let title = session_title(session);
                    if n == 0 {
                        session_header(ui, &title, session, &t, acts);
                        turn_list(ui, session, &t, acts);
                    } else {
                        let header = egui::CollapsingHeader::new(
                            RichText::new(&title).small().color(t.text_dim),
                        )
                        .id_salt(("agent-session", &session[0].session))
                        .default_open(false)
                        .show(ui, |ui| turn_list(ui, session, &t, acts));
                        session_menu(&header.header_response, session, acts);
                    }
                }
            });
        ui.separator();
    }
}

/// The turns, a slice per session, in the order they come (the latest session first).
fn sessions(turns: &[Turn]) -> Vec<&[Turn]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=turns.len() {
        if i == turns.len() || turns[i].session != turns[start].session {
            out.push(&turns[start..i]);
            start = i;
        }
    }
    out
}

/// What a session is called: its first prompt and when its last turn was.
fn session_title(session: &[Turn]) -> String {
    let first = session.first().map_or("", |t| t.label.as_str());
    let last = session.iter().map(|t| t.time).max().unwrap_or(0);
    let count = session.len();
    crate::i18n::message(
        "repo-workflows-agent-session-title",
        &[
            ("first", (shorten(first, 32)).into()),
            ("count", (count).into()),
            (
                "when",
                (crate::git::relative_time(last, crate::git::now())).into(),
            ),
        ],
    )
}

fn session_header(ui: &mut Ui, title: &str, session: &[Turn], t: &Tokens, acts: &mut Vec<Action>) {
    let r = ui.label(RichText::new(title).small().color(t.text_dim));
    session_menu(&r, session, acts);
    if ui
        .small_button(crate::i18n::text(
            "repo-workflows-agents-session-header-16959302",
        ))
        .clicked()
        && let Some(action) = aggregate_action(session)
    {
        acts.push(action);
    }
}

fn aggregate_action(session: &[Turn]) -> Option<Action> {
    let last = session.last()?;
    Some(Action::RepositoryTools(
        crate::repo::repository_tools::Act::AggregateTurn(
            last.session.clone(),
            last.commit.clone(),
        ),
    ))
}
fn session_menu(r: &egui::Response, session: &[Turn], acts: &mut Vec<Action>) {
    let Some(first) = session.first() else {
        return;
    };
    let id = first.session.clone();
    crate::menu_item::context(r)
        .style(crate::theme::menu_style)
        .show(|ui| {
            if crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-agents-session-menu-816f49fb",
            ))
            .icon("file-diff")
            .show(ui)
            .clicked()
                && let Some(action) = aggregate_action(session)
            {
                acts.push(action);
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-agents-session-menu-c3cada37",
            ))
            .icon("minus")
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-agents-session-menu-835a56f7",
            ))
            .clicked()
            {
                acts.push(Action::Agent(AgentAct::ForgetTurns(id.clone())));
            }
        });
}

fn turn_list(ui: &mut Ui, session: &[Turn], t: &Tokens, acts: &mut Vec<Action>) {
    egui::ScrollArea::vertical()
        .id_salt(("agent-turns", &session[0].session))
        .max_height(LIST_H)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for turn in session {
                turn_chip(ui, turn, t, acts);
            }
        });
}

/// What a turn changed, from the one before it.
fn turn_diff(turn: &Turn, label: &str) -> Action {
    Action::Compare {
        base: turn.parent.clone(),
        base_label: crate::i18n::message(
            "repo-workflows-agents-turn-diff-dc601e7b",
            &[("turn-number-1", (turn.number).clone().into())],
        ),
        target: Some(turn.commit.clone()),
        target_label: crate::i18n::message(
            "repo-workflows-agents-turn-diff-9021b8b9",
            &[
                ("turn-number-1", (turn.number).clone().into()),
                ("label", (label).into()),
            ],
        ),
        since_fork: false,
    }
}

fn turn_chip(ui: &mut Ui, turn: &Turn, t: &Tokens, acts: &mut Vec<Action>) {
    let label = shorten(&turn.label, LABEL_CHARS);
    let text = format!("{} · {label}", turn.number);
    let r = ui
        .add(
            egui::Button::selectable(false, RichText::new(text).size(12.0).color(t.text))
                .truncate(),
        )
        // Worked out only while hovered.
        .on_hover_ui(|ui| {
            ui.label(hover(turn));
        });
    if r.clicked() {
        acts.push(turn_diff(turn, &label));
    }
    crate::menu_item::context(&r)
        .style(crate::theme::menu_style)
        .show(|ui| {
            if crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-agents-turn-chip-a3b0355d",
            ))
            .icon("file-diff")
            .show(ui)
            .clicked()
            {
                acts.push(Action::RepositoryTools(
                    crate::repo::repository_tools::Act::Turn(
                        turn.parent.clone(),
                        turn.commit.clone(),
                        turn.session.clone(),
                    ),
                ));
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-agents-turn-chip-a246e9f1",
            ))
            .icon("command")
            .show(ui)
            .clicked()
            {
                acts.push(turn_diff(turn, &label));
            }
            if crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-agents-turn-chip-1d35c7ca",
            ))
            .icon("command")
            .show(ui)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-agents-turn-chip-eaf6cf58",
            ))
            .clicked()
            {
                acts.push(Action::Compare {
                    base: turn.commit.clone(),
                    base_label: crate::i18n::message(
                        "repo-workflows-agents-turn-chip-60c8b8ae",
                        &[("turn-number-1", (turn.number).clone().into())],
                    ),
                    target: None,
                    target_label: crate::i18n::text("repo-workflows-agents-turn-chip-ed5eb9a3")
                        .to_owned(),
                    since_fork: false,
                });
            }
        });
}

fn hover(turn: &Turn) -> String {
    let prompt = if turn.prompt.is_empty() {
        crate::i18n::text("repo-workflows-agents-hover-eae937e9")
    } else {
        turn.prompt.as_str()
    };
    let when = crate::git::relative_time(turn.time, crate::git::now());
    crate::i18n::message(
        "repo-workflows-agents-hover-0b4191eb",
        &[
            ("turn-number-1", (turn.number).clone().into()),
            ("when", (when).clone().into()),
            ("prompt", (prompt).into()),
        ],
    )
}

fn shorten(label: &str, max: usize) -> String {
    if label.chars().count() > max {
        format!("{}…", label.chars().take(max - 1).collect::<String>())
    } else {
        label.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(session: &str, number: u32, label: &str) -> Turn {
        Turn {
            session: session.to_owned(),
            number,
            label: label.to_owned(),
            ..Turn::default()
        }
    }

    #[test]
    fn turns_are_grouped_by_session_and_named_by_their_first_prompt() {
        let list = [
            turn("b", 1, "Fix the login"),
            turn("b", 2, "More"),
            turn("a", 1, "Old"),
        ];
        let groups = sessions(&list);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert!(session_title(groups[0]).starts_with("Fix the login · 2 turns · "));
        assert!(sessions(&[]).is_empty());
    }

    #[test]
    fn long_prompts_are_shortened() {
        assert_eq!(shorten("short", 48), "short");
        let long = "ü".repeat(60);
        assert_eq!(shorten(&long, 48).chars().count(), 48);
    }
}
