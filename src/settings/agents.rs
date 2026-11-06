//! The Agents page: how to add gitcraft to a coding agent as an MCP server, and whether agents
//! may change repositories or only read them.

use egui::{RichText, Ui};

use super::Settings;
use super::layout::{self, Filter, section};
use crate::theme::{self, Tokens};

impl Settings {
    pub(super) fn agents_ui(&mut self, ui: &mut Ui, f: &Filter) {
        let t = Tokens::get(ui.ctx());
        let program = crate::mcp::program();
        section(
            ui,
            f,
            "Add gitcraft to an agent",
            Some(
                "Run one of these once; it adds gitcraft for every project. The agent then starts \
                 gitcraft mcp in the repository it works in, and can read the graph, status, \
                 diffs, blame, conflicts, builds and pull requests.",
            ),
            |card| {
                for (agent, command) in crate::mcp::register_commands(&program) {
                    card.row(
                        agent,
                        "mcp register add claude codex command server ai agent",
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&command)
                                            .font(theme::mono(11.5))
                                            .color(t.text),
                                    )
                                    .wrap(),
                                );
                                if crate::icons::small_button(ui, "copy", "Copy the command")
                                    .clicked()
                                {
                                    ui.ctx().copy_text(command.clone());
                                }
                            });
                        },
                    );
                }
            },
        );
        section(
            ui,
            f,
            "Changes by agents",
            Some(
                "Each change is a step of the repository's undo list, marked with a robot. A \
                 push, deleting a branch, a hard reset, dropping a commit or staging a \
                 conflicted file waits for you to approve it.",
            ),
            |card| {
                card.row(
                "Agents may change repositories",
                "write tools allow off disable mcp ai robot approve approval permission read only",
                |ui| {
                    ui.checkbox(
                        &mut self.agent_writes,
                        "Stage, commit, branch, check out, stash and more",
                    )
                    .on_hover_text(
                        "Each runs in the repository's tab as a step of its undo list, marked \
                         with a robot: Ctrl+Z undoes it. A push, deleting a branch, a hard \
                         reset or dropping a commit waits for you to approve it first. \
                         Unticked, agents can only read.",
                    );
                },
            );
                card.row("Needs", "window open tab", |ui| {
                layout::note(
                    ui,
                    "Changes need the repository open in a tab of this window; reading works without it",
                );
            });
            },
        );
    }
}
