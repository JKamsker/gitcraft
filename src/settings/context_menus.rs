//! Optional tools in context menus. Hiding an entry never disables its command-palette action.

use egui::Ui;
use serde::{Deserialize, Serialize};

use super::Settings;
use super::layout::{Filter, section};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextMenus {
    pub diagnostics: bool,
    pub surgery: bool,
    pub rehearsal: bool,
    pub release: bool,
}

impl Settings {
    pub(super) fn context_menus_ui(&mut self, ui: &mut Ui, f: &Filter) {
        section(
            ui,
            f,
            "Optional tools",
            Some(
                "Keep common branch and commit actions close at hand. Optional tools stay available \
             in the command palette when hidden here; select a commit first to act on it.",
            ),
            |card| {
                for (label, keywords, enabled, text) in [
                    (
                        "History / diagnostics",
                        "bisect parallel test commits",
                        &mut self.context_menus.diagnostics,
                        "Show bisect and commit tests",
                    ),
                    (
                        "Commit surgery",
                        "rewrite split move changes",
                        &mut self.context_menus.surgery,
                        "Show split and move changes",
                    ),
                    (
                        "Branch rehearsal",
                        "rehearse interaction merge rebase",
                        &mut self.context_menus.rehearsal,
                        "Show rehearsal and interaction tools",
                    ),
                    (
                        "Releases",
                        "cut release tag publish",
                        &mut self.context_menus.release,
                        "Show Cut release",
                    ),
                ] {
                    card.row(label, keywords, |ui| {
                        ui.checkbox(enabled, text);
                    });
                }
            },
        );
    }
}
