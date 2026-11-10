//! Dispatch to page owners, including when search renders every page.

use super::layout::Filter;
use super::{
    Page, Settings, SettingsView, appearance_page, custom_commands, general_page, git_page,
    graph_page, identities_page, signing, terminal_page,
};
use egui::Ui;

impl Settings {
    /// One page of the settings, or what a search matches on it (see [`layout::Filter`]).
    pub(super) fn page_ui(&mut self, ui: &mut Ui, page: Page, f: &Filter, view: &mut SettingsView) {
        match page {
            Page::Profile => self.profile_ui(ui, f, view),
            Page::Network => self.network_ui(ui, f, false),
            Page::HttpsSignins => self.network_ui(ui, f, true),
            Page::General => general_page::render(self, ui, f, view),
            Page::Appearance => appearance_page::render(self, ui, f, view),
            Page::Graph => graph_page::render(self, ui, f),
            Page::Git => git_page::render(self, ui, f),
            Page::Ai => self.ai_ui(ui, f),
            Page::Signing => {
                let profile = self.profile();
                let overridden = profile.own_signing.then(|| profile.title().to_owned());
                signing::signing_ui(ui, f, overridden.as_deref());
            }
            Page::Terminal => terminal_page::render(self, ui, f),
            Page::Identities => identities_page::render(self, ui, f),
            Page::CustomCommands => custom_commands::page(self, ui, f),
            Page::Agents => self.agents_ui(ui, f),
            Page::ContextMenus => self.context_menus_ui(ui, f),
            Page::Shortcuts => {
                crate::keymap::editor::page(&mut self.key_bindings, ui, |command, keys| {
                    f.include_shortcut(&format!(
                        "{} {} {keys} keys keyboard",
                        command.id(),
                        command.title()
                    ))
                });
            }
        }
    }
}
