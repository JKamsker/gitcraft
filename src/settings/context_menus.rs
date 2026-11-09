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
            Some(crate::i18n::text("dev-baseline-optional-tip")),
            |card| {
                for (label, keywords, enabled, text) in [
                    (
                        "History / diagnostics",
                        "bisect parallel test commits",
                        &mut self.context_menus.diagnostics,
                        crate::i18n::text("dev-baseline-optional-0"),
                    ),
                    (
                        "Commit surgery",
                        "rewrite split move changes",
                        &mut self.context_menus.surgery,
                        crate::i18n::text("dev-baseline-optional-1"),
                    ),
                    (
                        "Branch rehearsal",
                        "rehearse interaction merge rebase",
                        &mut self.context_menus.rehearsal,
                        crate::i18n::text("dev-baseline-optional-2"),
                    ),
                    (
                        "Releases",
                        "cut release tag publish",
                        &mut self.context_menus.release,
                        crate::i18n::text("dev-baseline-optional-3"),
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
