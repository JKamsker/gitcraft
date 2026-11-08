//! The worktree setup recipe, shown and edited: files to copy or link from the main worktree,
//! variables, the setup command, and the script all that makes.

use egui::{RichText, Ui};

use super::AgentAct;
use crate::agent::Recipe;
use crate::agent::recipe::{env_lines, lines};
use crate::theme::{self, Tokens};

pub(crate) struct RecipeForm {
    copy: String,
    link: String,
    env: String,
    command: String,
    loading: bool,
    pub(super) saving: bool,
    pub(super) error: Option<String>,
}

impl RecipeForm {
    pub(super) fn loading() -> Self {
        Self {
            copy: String::new(),
            link: String::new(),
            env: String::new(),
            command: String::new(),
            loading: true,
            saving: false,
            error: None,
        }
    }

    pub(super) fn loaded(&mut self, recipe: Result<Recipe, String>) {
        self.loading = false;
        match recipe {
            Ok(recipe) => {
                self.copy = recipe.copy.join("\n");
                self.link = recipe.link.join("\n");
                self.env = recipe
                    .env
                    .iter()
                    .map(|v| format!("{}={}", v.name, v.value))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.command = recipe.command;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn recipe(&self) -> Recipe {
        Recipe {
            copy: lines(&self.copy),
            link: lines(&self.link),
            env: env_lines(&self.env),
            command: self.command.trim_end().to_owned(),
        }
    }

    /// Draws the editor; `in_linked`: the tab shows a linked worktree, which the recipe can be
    /// run on right away.
    pub(super) fn ui(&mut self, ui: &mut Ui, in_linked: bool) -> Option<AgentAct> {
        let t = Tokens::get(ui.ctx());
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-agents-ui-cbcf7990"))
                .font(theme::semibold(15.0)),
        );
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-agents-ui-f8894f23")).color(t.text_dim),
        );
        ui.add_space(8.0);
        if self.loading {
            ui.add(crate::spinner::Spinner::new());
            return cancel_only(ui);
        }
        let mut act = None;
        egui::ScrollArea::vertical()
            .max_height(ui.ctx().content_rect().height() * 0.6)
            .show(ui, |ui| {
                area(ui, &t, crate::i18n::text("repo-workflows-agents-ui-136eaccc"), &mut self.copy, ".env.local\nconfig/dev.json", 2, false);
                area(ui, &t, crate::i18n::text("repo-workflows-agents-ui-5c81d3ac"), &mut self.link, "node_modules", 2, false);
                area(ui, &t, crate::i18n::text("repo-workflows-agents-ui-cd1b3ecb"), &mut self.env, "PORT=$((3000 + GITCRAFT_WORKTREE_INDEX))\nCARGO_TARGET_DIR=target-$GITCRAFT_WORKTREE_INDEX", 2, true);
                area(ui, &t, crate::i18n::text("repo-workflows-agents-ui-74b240bb"), &mut self.command, "npm ci", 3, true);
                let recipe = self.recipe();
                for problem in recipe.problems() {
                    ui.colored_label(t.danger, problem);
                }
                egui::CollapsingHeader::new(RichText::new(crate::i18n::text("repo-workflows-agents-ui-7227fcf5")).color(t.text_dim))
                    .id_salt("recipe-script")
                    .show(ui, |ui| {
                        let mut script = recipe.script();
                        ui.add(
                            egui::TextEdit::multiline(&mut script)
                                .font(theme::mono(11.5))
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
            });
        if let Some(e) = &self.error {
            ui.colored_label(t.danger, e);
        }
        ui.add_space(10.0);
        let recipe = self.recipe();
        let valid = recipe.problems().is_empty() && !self.saving;
        ui.horizontal(|ui| {
            if crate::repo::primary_button(
                ui,
                crate::i18n::text("repo-workflows-agents-ui-1509f561"),
                valid,
                90.0,
            )
            .clicked()
            {
                act = Some(AgentAct::SaveRecipe(recipe.clone()));
            }
            let why_not = if !in_linked {
                Some(crate::i18n::text("repo-workflows-agents-ui-f37424d8"))
            } else if recipe.is_empty() {
                Some(crate::i18n::text("repo-workflows-agents-ui-beafb9a1"))
            } else if !valid {
                Some(crate::i18n::text("repo-workflows-agents-ui-6f521989"))
            } else {
                None
            };
            if ui
                .add_enabled(
                    why_not.is_none(),
                    egui::Button::new(crate::i18n::text("repo-workflows-agents-ui-1fe95550"))
                        .min_size(egui::vec2(80.0, 30.0)),
                )
                .on_hover_text(crate::i18n::text("repo-workflows-agents-ui-196e15ae"))
                .on_disabled_hover_text(why_not.unwrap_or_default())
                .clicked()
            {
                act = Some(AgentAct::RunRecipeHere(recipe));
            }
            if ui
                .add(
                    egui::Button::new(crate::i18n::text("repo-workflows-agents-ui-19766ed6"))
                        .min_size(egui::vec2(80.0, 30.0)),
                )
                .clicked()
            {
                act = Some(AgentAct::CloseDialog);
            }
        });
        act
    }
}

/// A labelled multi-line field.
fn area(
    ui: &mut Ui,
    t: &Tokens,
    label: &str,
    text: &mut String,
    hint: &str,
    rows: usize,
    mono: bool,
) {
    ui.label(RichText::new(label).color(t.text_dim).small());
    let mut edit = egui::TextEdit::multiline(text)
        .hint_text(hint)
        .desired_rows(rows)
        .desired_width(f32::INFINITY);
    if mono {
        edit = edit.font(theme::mono(12.0));
    }
    ui.add(edit);
    ui.add_space(6.0);
}

fn cancel_only(ui: &mut Ui) -> Option<AgentAct> {
    ui.add(
        egui::Button::new(crate::i18n::text(
            "repo-workflows-agents-cancel-only-19766ed6",
        ))
        .min_size(egui::vec2(80.0, 30.0)),
    )
    .clicked()
    .then_some(AgentAct::CloseDialog)
}
