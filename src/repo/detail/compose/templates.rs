//! The templates menu: start from the repository's `commit.template`, run its
//! `prepare-commit-msg` hook again, or pick a saved template (this repository's or every
//! repository's); save the message as one, delete one.

use egui::{RichText, Ui};

use super::store::{Scope, Store, Template};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::Tokens;

/// The "Save as template" form.
#[derive(Clone, Debug, Default)]
pub(super) struct SaveState {
    name: String,
    for_all: bool,
    error: Option<String>,
}

/// What was picked in the menu.
enum Pick {
    Text(String),
    Hook,
    Delete(Scope, String),
    Save,
}

impl RepoTab {
    pub(super) fn templates_button(&mut self, ui: &mut Ui) {
        let b = icons::small_button(ui, "list", crate::i18n::text("repo-core-1049"));
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        // Read only while the menu shows, not on every frame.
        let mut store: Option<Store> = None;
        let mut pick = None;
        let repo = self.repo_key().to_path_buf();
        let template = self.compose.template.clone();
        let has_message = !self.message_is_empty();
        let saving = &mut self.compose.saving;
        egui::Popup::menu(&b)
            .width(280.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.label(
                    RichText::new(crate::i18n::text("repo-core-1050"))
                        .color(t.text_faint)
                        .small(),
                );
                let tip = crate::i18n::text("repo-core-1051");
                if ui
                    .add_enabled(template.is_some(), egui::Button::new("commit.template"))
                    .on_hover_text(tip)
                    .on_disabled_hover_text(crate::i18n::text("repo-core-1053"))
                    .clicked()
                    && let Some(text) = &template
                {
                    pick = Some(Pick::Text(text.clone()));
                }
                if ui
                    .button(crate::i18n::text("repo-core-1054"))
                    .on_hover_text(crate::i18n::text("repo-core-1055"))
                    .clicked()
                {
                    pick = Some(Pick::Hook);
                }
                let store = store.insert(Store::load(&ctx));
                for (scope, title) in [
                    (Scope::Repo, crate::i18n::text("repo-core-1056")),
                    (Scope::Global, crate::i18n::text("repo-core-1057")),
                ] {
                    let list = store.templates(scope, &repo);
                    if list.is_empty() {
                        continue;
                    }
                    ui.separator();
                    ui.label(RichText::new(title).color(t.text_faint).small());
                    for template in list {
                        template_row(ui, scope, template, &mut pick);
                    }
                }
                ui.separator();
                save_form(ui, saving, has_message, &mut pick);
                if matches!(pick, Some(Pick::Text(_))) {
                    ui.close();
                }
            });
        let Some(mut store) = store else { return };
        match pick {
            Some(Pick::Text(text)) => {
                self.set_message(&text);
            }
            Some(Pick::Hook) => self.rerun_hook(&ctx),
            Some(Pick::Delete(scope, name)) => {
                store.delete_template(scope, &repo, &name);
                store.save(&ctx);
            }
            Some(Pick::Save) => {
                let saving = &mut self.compose.saving;
                let scope = if saving.for_all {
                    Scope::Global
                } else {
                    Scope::Repo
                };
                let text = super::join_message(&self.commit_summary, &self.commit_body);
                match store.save_template(scope, &repo, &saving.name, &text) {
                    Ok(()) => {
                        store.save(&ctx);
                        *saving = SaveState::default();
                    }
                    Err(e) => saving.error = Some(e),
                }
            }
            None => {}
        }
    }
}

/// A saved template: click to use it, × to delete it.
/// A saved template: click to use it, × (at the far end, away from the name) to delete it.
fn template_row(ui: &mut Ui, scope: Scope, template: &Template, pick: &mut Option<Pick>) {
    ui.horizontal(|ui| {
        if ui
            .add(egui::Button::new(&template.name).truncate())
            .on_hover_text(crate::i18n::message(
                "repo-core-1058",
                &[("text", template.text.to_string().into())],
            ))
            .clicked()
        {
            *pick = Some(Pick::Text(template.text.clone()));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let delete = crate::i18n::message(
                "repo-core-1059",
                &[("name", template.name.to_string().into())],
            );
            if icons::small_button(ui, "x", &delete).clicked() {
                *pick = Some(Pick::Delete(scope, template.name.clone()));
            }
            let first = template
                .text
                .trim_start()
                .lines()
                .next()
                .unwrap_or_default();
            let t = Tokens::get(ui.ctx());
            ui.add(egui::Label::new(RichText::new(first).color(t.text_faint).small()).truncate());
        });
    });
}

/// Name, where to keep it, and Save.
fn save_form(ui: &mut Ui, saving: &mut SaveState, has_message: bool, pick: &mut Option<Pick>) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        RichText::new(crate::i18n::text("repo-core-1060"))
            .color(t.text_faint)
            .small(),
    );
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut saving.name)
                .hint_text(crate::i18n::text("repo-core-1061"))
                .desired_width(150.0),
        );
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let save = ui
            .add_enabled(
                has_message,
                egui::Button::new(crate::i18n::text("repo-core-1062")),
            )
            .on_disabled_hover_text(crate::i18n::text("repo-core-1063"));
        if save.clicked() || (enter && has_message) {
            *pick = Some(Pick::Save);
        }
    });
    ui.checkbox(&mut saving.for_all, crate::i18n::text("repo-core-1064"))
        .on_hover_text(crate::i18n::text("repo-core-1065"));
    if let Some(e) = &saving.error {
        ui.label(RichText::new(e).color(t.danger).small());
    }
}
