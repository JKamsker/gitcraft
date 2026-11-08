//! The commit box's AI button: the tool picked in the settings writes the message from the
//! staged changes. A message already typed is never replaced without asking: the answer then
//! waits beside it, to use or to drop.

use std::time::Duration;

use egui::{RichText, Ui};

use super::{Running, coverage, elapsed};
use crate::ai::detect::{self, Look};
use crate::ai::prompt::split_message;
use crate::ai::{self, AiTool, Answer, Question, RunError};
use crate::icons;
use crate::repo::RepoTab;
use crate::settings::Settings;
use crate::theme::{self, Tokens};

/// The most lines of a waiting suggestion's description shown.
const PREVIEW_LINES: usize = 6;

impl RepoTab {
    /// The AI button, in the commit box's header. With a tool picked, a click writes the
    /// message and a right-click offers the tools; with none yet, a click offers them.
    /// `editable`: the message can be changed now (not while a rebase continues).
    pub(in crate::repo) fn ai_message_button(&mut self, ui: &mut Ui, editable: bool) {
        let settings = Settings::get(ui.ctx()).ai;
        let busy = self.ai.writing.is_some();
        let tip = if !editable {
            crate::i18n::text("repo-workflows-ai-ai-message-button-9553c66f").to_owned()
        } else if busy {
            crate::i18n::text("repo-workflows-ai-ai-message-button-04254124").to_owned()
        } else if settings.enabled() && self.amend {
            crate::i18n::message(
                "repo-workflows-ai-ai-message-button-05060623",
                &[(
                    "settings-tool-short-1",
                    (settings.tool.short()).clone().into(),
                )],
            )
        } else if settings.enabled() {
            crate::i18n::message(
                "repo-workflows-ai-ai-message-button-33272cf9",
                &[(
                    "settings-tool-short-1",
                    (settings.tool.short()).clone().into(),
                )],
            )
        } else {
            crate::i18n::text("repo-workflows-ai-ai-message-button-a1323968").to_owned()
        };
        let resp = ui
            .add_enabled_ui(editable && !busy, |ui| {
                icons::small_button(ui, "sparkles", &tip)
            })
            .inner;
        if resp.clicked() && settings.enabled() {
            self.start_message(ui.ctx());
        }
        let popup = if settings.enabled() {
            crate::menu_item::context(&resp)
        } else {
            egui::Popup::menu(&resp)
        };
        let mut chosen = None;
        popup
            .style(theme::menu_style)
            .show(|ui| chosen = self.tool_menu(ui, settings.tool));
        if let Some(tool) = chosen {
            let mut all = Settings::get(ui.ctx());
            all.ai.tool = tool;
            all.set(ui.ctx());
            if all.ai.enabled() {
                self.start_message(ui.ctx());
            }
        }
    }

    /// The tools to pick, as found on the repository's machine; the one picked, if any.
    fn tool_menu(&mut self, ui: &mut Ui, current: AiTool) -> Option<AiTool> {
        let t = Tokens::get(ui.ctx());
        self.look_for_tools(ui.ctx());
        let settings = Settings::get(ui.ctx()).ai;
        let place = crate::remote::parse(&self.path).map_or_else(
            || crate::i18n::text("repo-workflows-ai-tool-menu-e223809d").to_owned(),
            |t| t.host.label(),
        );
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-ai-tool-menu-72f76d6c",
                &[("place", (place).clone().into())],
            ))
            .color(t.text_faint)
            .small(),
        );
        let mut chosen = None;
        for tool in AiTool::CHOICES {
            let found = self.ai.tools.has(tool);
            let no_command = tool == AiTool::Custom && settings.custom_command.trim().is_empty();
            let text = match found {
                Some(true) if no_command => crate::i18n::message(
                    "repo-workflows-ai-tool-menu-671a1cd9",
                    &[("tool-label-1", (tool.label()).clone().into())],
                ),
                Some(true) => tool.label().to_owned(),
                Some(false) => crate::i18n::message(
                    "repo-workflows-ai-tool-menu-9b1177a4",
                    &[("tool-label-1", (tool.label()).clone().into())],
                ),
                None if matches!(self.ai.tools, Look::Failed(_)) => tool.label().to_owned(),
                None => crate::i18n::message(
                    "repo-workflows-ai-tool-menu-0c0aa296",
                    &[("tool-label-1", (tool.label()).clone().into())],
                ),
            };
            let refused = if no_command {
                Some(crate::i18n::text("repo-workflows-ai-tool-menu-77cbb6fb"))
            } else if found == Some(false) {
                Some(crate::i18n::text("repo-workflows-ai-tool-menu-9950df89"))
            } else {
                None
            };
            if crate::menu_item::MenuItem::new(text)
                .icon("sparkles")
                .selected(current == tool)
                .refused(refused)
                .show(ui)
                .clicked()
            {
                chosen = Some(tool);
            }
        }
        if let Look::Failed(e) = &self.ai.tools {
            ui.label(RichText::new(e).color(t.danger).small());
        }
        if matches!(self.ai.tools, Look::Found(_) | Look::Failed(_))
            && crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-ai-tool-menu-d36feef0",
            ))
            .icon("refresh-cw")
            .show(ui)
            .on_hover_text(crate::i18n::text("repo-workflows-ai-tool-menu-1440bda3"))
            .clicked()
        {
            self.ai.tools = Look::NotYet;
        }
        ui.separator();
        if current != AiTool::Off
            && crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-ai-tool-menu-06f0e210",
            ))
            .icon("x")
            .show(ui)
            .clicked()
        {
            chosen = Some(AiTool::Off);
        }
        ui.label(
            RichText::new(crate::i18n::text("repo-workflows-ai-tool-menu-5e2d9c98"))
                .color(t.text_faint)
                .small(),
        );
        chosen
    }

    /// Looks for the tools on the repository's machine, once.
    fn look_for_tools(&mut self, ctx: &egui::Context) {
        if self.ai.tools != Look::NotYet {
            return;
        }
        self.ai.tools = Look::Looking;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || detect::installed(&repo),
            |tab, found, _| {
                tab.ai.tools = match found {
                    Ok(found) => Look::Found(found),
                    Err(e) => Look::Failed(e),
                };
            },
        );
    }

    /// Has the tool the settings pick write a message for what is staged.
    pub(in crate::repo) fn start_message(&mut self, ctx: &egui::Context) {
        let settings = Settings::get(ctx).ai;
        self.cancel_message();
        let running = self.ai.start(settings.tool);
        let (id, cancel) = (running.id, running.cancel.clone());
        self.ai.writing = Some(running);
        self.ai.note = None;
        self.ai.suggestion = None;
        let repo = self.path.clone();
        let question = Question::CommitMessage { amend: self.amend };
        self.then(
            ctx,
            move || ai::ask(&repo, &settings, &question, ai::TIMEOUT, &cancel),
            move |tab, answer, _| tab.message_answered(id, answer),
        );
    }

    /// After a commit: what was written for it, or is being written, is done with.
    pub(in crate::repo) fn ai_committed(&mut self) {
        self.cancel_message();
        self.ai.suggestion = None;
        self.ai.note = None;
    }

    /// Stops the message being written, if one is.
    pub(in crate::repo) fn cancel_message(&mut self) {
        if let Some(running) = self.ai.writing.take() {
            running.cancel.cancel();
        }
    }

    /// The answer to request `id`: into the box when it is empty, else waiting beside it.
    pub(in crate::repo) fn message_answered(&mut self, id: u64, answer: Result<Answer, RunError>) {
        if self.ai.writing.as_ref().map(|r| r.id) != Some(id) {
            return;
        }
        self.ai.writing = None;
        let answer = match answer {
            Ok(answer) => answer,
            Err(RunError::Cancelled) => return,
            Err(RunError::Failed(e)) => {
                self.ai.note = Some((e, true));
                return;
            }
        };
        let (summary, body) = split_message(&answer.text);
        if summary.is_empty() {
            self.ai.note = Some((
                crate::i18n::text("repo-workflows-ai-message-answered-00f5dcad").to_owned(),
                true,
            ));
            return;
        }
        self.ai.note = coverage(answer.files, answer.left_out).map(|n| (n, false));
        if self.commit_summary.trim().is_empty() && self.commit_body.trim().is_empty() {
            self.commit_summary = summary;
            self.commit_body = body;
        } else {
            self.ai.suggestion = Some((summary, body));
        }
    }

    /// Under the message: the tool at work, the message it wrote when one was typed already,
    /// what went wrong.
    pub(in crate::repo) fn ai_message_status(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        if self.ai.writing.as_ref().is_some_and(|r| writing_row(ui, r)) {
            self.cancel_message();
        }
        let choice = self
            .ai
            .suggestion
            .as_ref()
            .and_then(|(summary, body)| suggestion_card(ui, summary, body));
        match (choice, self.ai.suggestion.take()) {
            (Some(true), Some((summary, body))) => {
                self.commit_summary = summary;
                self.commit_body = body;
            }
            (None, waiting) => self.ai.suggestion = waiting,
            _ => {}
        }
        if let Some((text, error)) = &self.ai.note {
            let mut dismissed = false;
            ui.horizontal(|ui| {
                let color = if *error { t.danger } else { t.text_faint };
                ui.add(egui::Label::new(RichText::new(text).color(color).small()).wrap());
                dismissed = icons::small_button(
                    ui,
                    "x",
                    crate::i18n::text("repo-workflows-ai-ai-message-status-48845bff"),
                )
                .clicked();
            });
            if dismissed {
                self.ai.note = None;
            }
        }
    }
}

/// The tool at work and a button to stop it; `true` when it was clicked.
fn writing_row(ui: &mut Ui, running: &Running) -> bool {
    let t = Tokens::get(ui.ctx());
    ui.ctx().request_repaint_after(Duration::from_secs(1));
    ui.horizontal(|ui| {
        ui.add(crate::spinner::Spinner::new());
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-ai-writing-row-ea07a4e5",
                &[
                    (
                        "running-tool-short-1",
                        (running.tool.short()).clone().into(),
                    ),
                    (
                        "elapsed-running-started-2",
                        (elapsed(running.started)).clone().into(),
                    ),
                ],
            ))
            .color(t.text_dim)
            .small(),
        );
        ui.button(crate::i18n::text("repo-workflows-ai-writing-row-19766ed6"))
            .clicked()
    })
    .inner
}

/// A message the tool wrote while one was typed: `Some(true)` to use it, `Some(false)` to
/// keep the one typed.
fn suggestion_card(ui: &mut Ui, summary: &str, body: &str) -> Option<bool> {
    let t = Tokens::get(ui.ctx());
    let mut choice = None;
    egui::Frame::new()
        .fill(t.field)
        .stroke(egui::Stroke::new(1.0, t.field_border))
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(crate::i18n::text(
                    "repo-workflows-ai-suggestion-card-e3bbc8d9",
                ))
                .color(t.text_faint)
                .small(),
            );
            ui.add(egui::Label::new(RichText::new(summary).font(theme::semibold(12.5))).wrap());
            if !body.is_empty() {
                let lines: Vec<&str> = body.lines().collect();
                let mut shown = lines[..lines.len().min(PREVIEW_LINES)].join("\n");
                if lines.len() > PREVIEW_LINES {
                    shown.push_str("\n…");
                }
                ui.add(egui::Label::new(RichText::new(shown).color(t.text_dim).small()).wrap());
            }
            ui.horizontal(|ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-ai-suggestion-card-57a64561",
                    ))
                    .on_hover_text(crate::i18n::text(
                        "repo-workflows-ai-suggestion-card-5a6b5767",
                    ))
                    .clicked()
                {
                    choice = Some(true);
                }
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-ai-suggestion-card-0335c833",
                    ))
                    .clicked()
                {
                    choice = Some(false);
                }
            });
        });
    choice
}
