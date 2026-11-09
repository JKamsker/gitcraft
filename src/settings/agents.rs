//! The Agents page: how to add gitcraft to a coding agent as an MCP server, and whether agents
//! may change repositories or only read them.

use egui::{RichText, Ui};

use super::Settings;
use super::layout::{self, Filter, section};
use crate::theme::{self, Tokens};

impl Settings {
    pub(super) fn agents_ui(&mut self, ui: &mut Ui, f: &Filter) {
        let t = Tokens::get(ui.ctx());
        section(
            ui,
            f,
            "Sidebar",
            Some(crate::i18n::text("dev-baseline-agents-tip")),
            |card| {
                card.row(
                    "Show Agents section",
                    "sidebar feature flag tasks visibility robot",
                    |ui| {
                        ui.checkbox(
                            &mut self.show_agents,
                            crate::i18n::text("dev-baseline-agents-show"),
                        );
                    },
                );
            },
        );
        let program = crate::mcp::program();
        section(
            ui,
            f,
            "Add gitcraft to an agent",
            Some(crate::i18n::text(
                "interaction-settings-agent-register-intro",
            )),
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
                                if crate::icons::small_button(
                                    ui,
                                    "copy",
                                    crate::i18n::text("interaction-settings-copy-command"),
                                )
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
            Some(crate::i18n::text(
                "interaction-settings-agent-changes-intro",
            )),
            |card| {
                card.row(
                "Agents may change repositories",
                "write tools allow off disable mcp ai robot approve approval permission read only",
                |ui| {
                    ui.checkbox(
                        &mut self.agent_writes,
                        crate::i18n::text("interaction-settings-agent-writes"),
                    )
                    .on_hover_text(
                        crate::i18n::text("interaction-settings-agent-writes-tip"),
                    );
                },
            );
                card.row("Needs", "window open tab", |ui| {
                    layout::note(ui, crate::i18n::text("interaction-settings-agent-needs"));
                });
            },
        );
    }
}
