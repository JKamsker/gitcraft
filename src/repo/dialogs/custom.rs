//! The dialogs of custom commands: the prompts and confirmation asked before one runs, and the
//! review of a repository's own commands before they are trusted.

use egui::{RichText, Ui};

use super::{Outcome, buttons, cancel_button, field, title};
use crate::custom_commands::{self as cc, CustomCommand, PromptKind, RepoFile, Scope};
use crate::repo::Action;
use crate::repo::custom_commands::CustomAct;
use crate::theme::{self, Tokens};

/// Branches a branch prompt lists at most; typing narrows them down.
const MAX_LISTED: usize = 200;

pub enum CustomDialog {
    /// Ask a command's prompts (and whether to run it).
    Ask {
        command: Box<CustomCommand>,
        scope: Scope,
        /// An answer per prompt, in their order.
        answers: Vec<String>,
        /// The branches a branch prompt can pick.
        branches: Vec<String>,
        focus: bool,
    },
    /// Show a repository's command file (of the repository named so), to trust it.
    Review { repo: String, file: RepoFile },
}

impl CustomDialog {
    pub fn ask(command: CustomCommand, scope: Scope, branches: &[String]) -> Self {
        let answers = command.prompts.iter().map(|p| p.default.clone()).collect();
        CustomDialog::Ask {
            command: Box::new(command),
            scope,
            answers,
            branches: branches.to_vec(),
            focus: true,
        }
    }

    pub fn review(repo: String, file: RepoFile) -> Self {
        CustomDialog::Review { repo, file }
    }
}

pub(super) fn custom(ui: &mut Ui, enter: bool, dialog: &mut CustomDialog) -> Outcome {
    match dialog {
        CustomDialog::Ask {
            command,
            scope,
            answers,
            branches,
            focus,
        } => ask(ui, enter, command, scope, answers, branches, focus),
        CustomDialog::Review { repo, file } => review(ui, repo, file),
    }
}

/// The command's prompts, what it is about, its command line, and Run.
fn ask(
    ui: &mut Ui,
    enter: bool,
    command: &CustomCommand,
    scope: &Scope,
    answers: &mut [String],
    branches: &[String],
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let about = about(scope);
    title(ui, command.name.trim(), &about, &t);
    for (i, (prompt, answer)) in command.prompts.iter().zip(answers.iter_mut()).enumerate() {
        let label = if prompt.label.trim().is_empty() {
            prompt.variable.as_str()
        } else {
            prompt.label.trim()
        };
        ui.label(label);
        let mut take_focus = *focus && i == 0;
        field(
            ui,
            answer,
            &format!("${}", prompt.variable),
            &mut take_focus,
        );
        if prompt.kind == PromptKind::Branch {
            branch_list(ui, i, answer, branches);
        }
        ui.add_space(6.0);
    }
    *focus = false;
    ui.label(
        RichText::new(crate::i18n::text("repo-core-1287"))
            .color(t.text_dim)
            .small(),
    );
    command_line(ui, "custom-ask", &command.command);
    ui.add_space(12.0);
    // Enter runs it only from an answer typed: a confirmation alone wants a click, so that a
    // stray Enter never runs a command.
    let enter = enter && !command.prompts.is_empty();
    buttons(ui, crate::i18n::text("repo-core-1212"), true, enter, || {
        Action::Custom(CustomAct::Run {
            command: Box::new(command.clone()),
            scope: scope.clone(),
            answers: answers.to_vec(),
        })
    })
}

/// "abc12345 · main · origin · src/x.rs": what the command is about, for under its name.
fn about(scope: &Scope) -> String {
    let short: String = scope.sha.chars().take(8).collect();
    let parts: Vec<&str> = [
        short.as_str(),
        scope.branch.as_str(),
        scope.remote.as_str(),
        scope.file.as_str(),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();
    if parts.is_empty() {
        crate::i18n::text("repo-core-1289").to_owned()
    } else {
        parts.join(" · ")
    }
}

/// The branches matching what is typed in `answer`, to pick one.
fn branch_list(ui: &mut Ui, i: usize, answer: &mut String, branches: &[String]) {
    let typed = answer.to_lowercase();
    let matching: Vec<&String> = branches
        .iter()
        .filter(|b| b.to_lowercase().contains(&typed) && **b != *answer)
        .take(MAX_LISTED)
        .collect();
    if matching.is_empty() {
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt(("custom-branches", i))
        .max_height(110.0)
        .show(ui, |ui| {
            for b in matching {
                if ui.selectable_label(false, b.as_str()).clicked() {
                    answer.clone_from(b);
                }
            }
        });
}

/// A command line, as it will run, in a box.
fn command_line(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let t = Tokens::get(ui.ctx());
    egui::Frame::new()
        .fill(t.field)
        .corner_radius(t.radius)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt(("custom-command-line", id))
                .max_height(120.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(text).font(theme::mono(12.0))).wrap());
                });
        });
}

/// The repository's commands, each with what it asks, whether it asks before running, and
/// its command line; and whether to trust them.
fn review(ui: &mut Ui, repo: &str, file: &RepoFile) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.set_width(520.0);
    title(
        ui,
        crate::i18n::text("repo-core-1292"),
        &crate::i18n::message(
            "repo-core-1293",
            &[
                ("REPO-FILE", format!("{}", cc::REPO_FILE).into()),
                ("repo", format!("{}", repo).into()),
            ],
        ),
        &t,
    );
    let commands = match &file.commands {
        Ok(list) => list,
        Err(e) => {
            ui.colored_label(t.danger, e);
            ui.add_space(12.0);
            return if cancel_button(ui) {
                Outcome::Cancel
            } else {
                Outcome::Open
            };
        }
    };
    egui::ScrollArea::vertical()
        .id_salt("custom-review")
        .max_height(360.0)
        .show(ui, |ui| {
            if commands.is_empty() {
                ui.label(RichText::new(crate::i18n::text("repo-core-1295")).color(t.text_dim));
            }
            for (i, c) in commands.iter().enumerate() {
                ui.add(
                    egui::Label::new(RichText::new(c.name.trim()).font(theme::semibold(13.0)))
                        .wrap(),
                );
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-core-1296",
                        &[
                            ("label", format!("{}", c.placement.label()).into()),
                            ("label2", format!("{}", c.output.label()).into()),
                        ],
                    ))
                    .color(t.text_faint)
                    .small(),
                );
                if !c.confirm && c.prompts.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::text("repo-core-1297"))
                            .color(t.danger)
                            .small(),
                    );
                }
                for p in &c.prompts {
                    let mut asks = crate::i18n::message(
                        "repo-core-1298",
                        &[
                            ("variable", format!("{}", p.variable).into()),
                            ("label", format!("{}", p.kind.label()).into()),
                        ],
                    );
                    if !p.label.trim().is_empty() {
                        asks.push_str(&format!(": {}", p.label.trim()));
                    }
                    if !p.default.is_empty() {
                        asks.push_str(&crate::i18n::message(
                            "repo-core-1299",
                            &[("default", format!("{}", p.default).into())],
                        ));
                    }
                    ui.label(RichText::new(asks).color(t.text_dim).small());
                }
                command_line(ui, ("custom-review", i), &c.command);
                ui.add_space(8.0);
            }
        });
    ui.add_space(8.0);
    let (digest, count) = (file.digest.clone(), commands.len());
    buttons(
        ui,
        crate::i18n::text("repo-core-1300"),
        true,
        false,
        move || {
            Action::Custom(CustomAct::Trust {
                digest,
                commands: count,
            })
        },
    )
}
