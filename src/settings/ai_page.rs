//! Settings → AI: the tool that writes commit messages and reviews branches, its prompts, and
//! how much of a diff it is sent.

use std::sync::Mutex;

use egui::{RichText, Ui};

use super::Settings;
use super::layout::{self, Filter, section};
use crate::ai::AiTool;
use crate::ai::detect::{self, Look};
use crate::ai::settings::{DEFAULT_COMMIT_PROMPT, DEFAULT_REVIEW_PROMPT, MAX_DIFF_KB_RANGE};
use crate::theme::Tokens;

/// The tools found on this machine, looked for once, the first time the page shows.
static HERE: Mutex<Look> = Mutex::new(Look::NotYet);

/// What is known of the tools on this machine; starts looking when nothing is.
fn tools_here(ctx: &egui::Context) -> Look {
    let mut look = HERE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *look == Look::NotYet {
        *look = Look::Looking;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let found = match detect::installed(&std::env::temp_dir()) {
                Ok(found) => Look::Found(found),
                Err(e) => Look::Failed(e),
            };
            *HERE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = found;
            ctx.request_repaint();
        });
    }
    look.clone()
}

/// "claude, gh copilot": the tools found, for a note.
fn names(found: &[AiTool]) -> String {
    found
        .iter()
        .map(|t| t.short())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Height of the prompt fields, in rows.
const PROMPT_ROWS: usize = 7;

impl Settings {
    pub(super) fn ai_ui(&mut self, ui: &mut Ui, f: &Filter) {
        // A search draws every page: only the page itself, shown, starts the look.
        let look = if f.searching() {
            HERE.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        } else {
            tools_here(ui.ctx())
        };
        section(
            ui,
            f,
            "AI",
            Some(crate::i18n::text("interaction-tools-ai-intro")),
            |card| {
                card.row("Tool", "ai claude codex copilot gh llm off", |ui| {
                    egui::ComboBox::from_id_salt("settings-ai-tool")
                        .width(layout::fit(ui, super::COMBO_WIDTH + 40.0))
                        .selected_text(self.ai.tool.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.ai.tool,
                                AiTool::Off,
                                crate::i18n::text("interaction-tools-off-choice"),
                            );
                            for tool in AiTool::CHOICES {
                                let label = match look.has(tool) {
                                    Some(false) => crate::i18n::message(
                                        "interaction-tools-not-found",
                                        &[("tool", tool.label().into())],
                                    ),
                                    _ => tool.label().to_owned(),
                                };
                                ui.selectable_value(&mut self.ai.tool, tool, label);
                            }
                        });
                    let note = match &look {
                        Look::NotYet | Look::Looking => {
                            crate::i18n::text("interaction-tools-looking").into()
                        }
                        Look::Found(found) if found.is_empty() => {
                            crate::i18n::text("interaction-tools-none-found").into()
                        }
                        Look::Found(found) => crate::i18n::message(
                            "interaction-tools-found",
                            &[("tools", names(found).into())],
                        ),
                        Look::Failed(e) => e.clone(),
                    };
                    ui.horizontal(|ui| {
                        layout::note(ui, note);
                        if matches!(look, Look::Found(_) | Look::Failed(_))
                            && ui
                                .small_button(crate::i18n::text("interaction-tools-again"))
                                .clicked()
                        {
                            *HERE
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) = Look::NotYet;
                        }
                    });
                });
                if self.ai.tool == AiTool::Custom {
                    card.row_hint(
                        "Custom command",
                        crate::i18n::text("interaction-tools-reads-prompt"),
                        "ai custom command ollama llm script",
                        |ui| {
                            ui.add(
                                layout::field(&mut self.ai.custom_command)
                                    .hint_text("ollama run llama3")
                                    .desired_width(layout::fit(ui, super::FIELD_WIDTH)),
                            )
                            .on_hover_text(crate::i18n::text("interaction-tools-custom-help"));
                            if self.ai.custom_command.trim().is_empty() {
                                layout::note(ui, crate::i18n::text("interaction-tools-off-until"));
                            }
                        },
                    );
                }
                card.row_hint(
                    "Largest diff sent",
                    crate::i18n::text("interaction-tools-rest-named"),
                    "ai size limit kb truncate diff",
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.ai.max_diff_kb)
                                .range(MAX_DIFF_KB_RANGE)
                                .custom_formatter(|value, _| crate::i18n::number(value))
                                .custom_parser(super::parse_localized_number)
                                .suffix(" KB"),
                        );
                    },
                );
                card.row_hint(
                    "Commit prompt",
                    "{conventions}, {left_out}, {diff}",
                    "ai prompt template commit message conventional",
                    |ui| {
                        prompt_field(ui, &mut self.ai.commit_prompt, DEFAULT_COMMIT_PROMPT);
                        layout::note(ui, crate::i18n::text("interaction-tools-conventions"));
                    },
                );
                card.row_hint(
                    "Review prompt",
                    "{branch}, {base}, {commits}, {left_out}, {diff}",
                    "ai prompt template review branch",
                    |ui| {
                        prompt_field(ui, &mut self.ai.review_prompt, DEFAULT_REVIEW_PROMPT);
                    },
                );
            },
        );
    }
}

/// A prompt's field, with a button that puts the default one back.
fn prompt_field(ui: &mut Ui, text: &mut String, default: &str) {
    ui.add(
        egui::TextEdit::multiline(text)
            .desired_rows(PROMPT_ROWS)
            .desired_width(layout::fit(ui, super::FIELD_WIDTH + 140.0))
            .font(egui::TextStyle::Monospace),
    );
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                text != default,
                egui::Button::new(crate::i18n::text("interaction-tools-restore")),
            )
            .clicked()
        {
            *text = default.to_owned();
        }
        if !text.contains("{diff}") {
            ui.label(
                RichText::new(crate::i18n::text("interaction-tools-no-diff"))
                    .color(t.text_faint)
                    .small(),
            );
        }
    });
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn prompt_controls_translate_without_changing_template_tokens_or_default_payload() {
        crate::i18n::set_language("de");
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, text: &mut String| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                prompt_field(ui, text, DEFAULT_COMMIT_PROMPT);
            },
            "User-owned instruction 日本語".to_owned(),
        );
        h.run();
        h.get_by_label("Kein {diff}: Die Änderungen werden am Ende angefügt");
        h.get_by_label("Standard wiederherstellen").click();
        h.run();
        assert_eq!(h.state(), DEFAULT_COMMIT_PROMPT);
        assert!(h.state().contains("{conventions}"));
        assert!(h.state().contains("{diff}"));
        assert_eq!(AiTool::Custom.label(), "Eigener Befehl");
        assert_eq!(
            serde_json::to_string(&AiTool::Custom).unwrap(),
            "\"Custom\""
        );
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Restore default");
        assert_eq!(h.state(), DEFAULT_COMMIT_PROMPT);
    }
}
