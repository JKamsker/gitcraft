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
        let look = tools_here(ui.ctx());
        section(
            ui,
            f,
            "AI",
            Some(
                "A button in the commit box asks an AI command-line tool you already have for a \
                 message, and Review this branch in a branch's menu asks it for a review. It \
                 runs on the repository's machine (here, in WSL or over SSH) with its own \
                 sign-in: gitcraft keeps no API key. It is sent the staged changes (or what the \
                 branch adds), your latest commit summaries and the prompt below.",
            ),
            |card| {
                card.row("Tool", "ai claude codex copilot gh llm off", |ui| {
                    egui::ComboBox::from_id_salt("settings-ai-tool")
                        .width(layout::fit(ui, super::COMBO_WIDTH + 40.0))
                        .selected_text(self.ai.tool.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.ai.tool, AiTool::Off, "Off");
                            for tool in AiTool::CHOICES {
                                let label = match look.has(tool) {
                                    Some(false) => format!("{} — not found here", tool.label()),
                                    _ => tool.label().to_owned(),
                                };
                                ui.selectable_value(&mut self.ai.tool, tool, label);
                            }
                        });
                    let note = match &look {
                        Look::NotYet | Look::Looking => "Looking for them on this machine…".into(),
                        Look::Found(found) if found.is_empty() => {
                            "None of claude, codex or gh copilot is on this machine's PATH".into()
                        }
                        Look::Found(found) => format!("On this machine: {}", names(found)),
                        Look::Failed(e) => e.clone(),
                    };
                    layout::note(ui, note);
                });
                if self.ai.tool == AiTool::Custom {
                    card.row_hint(
                        "Custom command",
                        "Reads the prompt, prints the answer",
                        "ai custom command ollama llm script",
                        |ui| {
                            ui.add(
                                layout::field(&mut self.ai.custom_command)
                                    .hint_text("ollama run llama3")
                                    .desired_width(layout::fit(ui, super::FIELD_WIDTH)),
                            )
                            .on_hover_text(
                                "Run by sh (cmd on Windows) in the repository's folder, on its \
                                 machine. The prompt comes on its input; what it prints is the \
                                 answer.",
                            );
                            if self.ai.custom_command.trim().is_empty() {
                                layout::note(ui, "Off until a command is set");
                            }
                        },
                    );
                }
                card.row_hint(
                    "Most of a diff sent",
                    "The rest is named, with its line counts",
                    "ai size limit kb truncate diff",
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut self.ai.max_diff_kb)
                                .range(MAX_DIFF_KB_RANGE)
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
                        layout::note(
                            ui,
                            "{conventions} says how the repository's latest summaries are \
                             written (Conventional Commits when most of them are).",
                        );
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
            .add_enabled(text != default, egui::Button::new("Restore default"))
            .clicked()
        {
            *text = default.to_owned();
        }
        if !text.contains("{diff}") {
            ui.label(
                RichText::new("No {diff}: the changes go at the end")
                    .color(t.text_faint)
                    .small(),
            );
        }
    });
}
