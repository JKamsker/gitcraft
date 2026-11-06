//! Hooks: the folder git runs them from, which hooks are active and what runs each, what keeps
//! one from running (with a one-click fix where there is one), the hook managers, and what the
//! commands that ran hooks printed.

use egui::{RichText, Ui};

use super::DoctorAct;
use super::hooks::{HooksAct, HooksState};
use super::page::{card, code, dim, faint, reading};
use crate::git::doctor::hook_log::Run;
use crate::git::doctor::hooks::{Hook, Hooks, Problem};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// Most lines of a run's output shown.
const OUTPUT_LINES: usize = 40;

fn act(a: HooksAct) -> Action {
    Action::Doctor(DoctorAct::Hooks(a))
}

pub(super) fn ui(ui: &mut Ui, state: &HooksState, locked: bool, acts: &mut Vec<Action>) {
    faint(
        ui,
        "Hooks are programs git runs at points of an operation: before a commit, before a push, \
         after a checkout. A hook that fails stops the operation, and one that can't run is \
         skipped or fails with a cryptic message.",
    );
    let t = Tokens::get(ui.ctx());
    match &state.fixed {
        Some(Ok(done)) => {
            ui.add_space(6.0);
            ui.label(RichText::new(done).color(t.success));
        }
        Some(Err(e)) => {
            ui.add_space(6.0);
            ui.label(RichText::new(e).color(t.danger));
        }
        None => {}
    }
    if let Some(found) = reading(ui, &state.found) {
        folder_card(ui, found);
        hooks_card(ui, found, state.fixing || locked, acts);
    }
    output_card(ui, &state.runs);
}

/// The folder git runs hooks from, and the tools that manage them.
fn folder_card(ui: &mut Ui, found: &Hooks) {
    let t = Tokens::get(ui.ctx());
    card(ui, "Hooks folder", None, |ui| {
        ui.label(code(&found.folder).color(t.text));
        match &found.hooks_path {
            Some(path) => dim(
                ui,
                format!("Set by core.hooksPath ({path}), instead of .git/hooks."),
            ),
            None => dim(
                ui,
                "The repository's own (.git/hooks): no core.hooksPath is set.",
            ),
        }
        if found.missing {
            ui.label(RichText::new("This folder doesn't exist: no hook runs.").color(t.warning));
        }
        if found.samples > 0 {
            faint(
                ui,
                format!(
                    "{} .sample files: examples git ships, never run.",
                    found.samples
                ),
            );
        }
        for m in &found.managers {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, m.label(), t.accent);
                dim(
                    ui,
                    format!(
                        "manages the hooks here: change them in {}, which it installs from.",
                        m.configured_in()
                    ),
                );
            });
        }
    });
}

/// The active hooks, what runs each, and their problems.
fn hooks_card(ui: &mut Ui, found: &Hooks, locked: bool, acts: &mut Vec<Action>) {
    let title = match found.active.len() {
        0 => "No active hooks".to_owned(),
        1 => "1 hook".to_owned(),
        n => format!("{n} hooks"),
    };
    card(ui, &title, None, |ui| {
        if found.active.is_empty() {
            faint(ui, "No hook runs: every operation goes ahead without one.");
            return;
        }
        for (i, hook) in found.active.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            hook_row(ui, hook, locked, acts);
        }
    });
}

fn hook_row(ui: &mut Ui, hook: &Hook, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(code(&hook.name).color(t.text));
        if let Some(on) = hook.runs_on {
            ui.label(RichText::new(format!("runs on {on}")).color(t.text_dim));
        }
        if let Some(i) = &hook.interpreter {
            widgets::chip(ui, i, t.text_dim);
        }
        if hook.problems.is_empty() {
            widgets::chip(ui, "ready", t.success);
        }
    });
    for problem in &hook.problems {
        ui.horizontal_wrapped(|ui| {
            let color = match problem {
                Problem::UnknownName => t.text_faint,
                _ => t.warning,
            };
            ui.add(egui::Label::new(RichText::new(problem.describe()).color(color)).wrap());
            let fix = match problem {
                Problem::NotExecutable => Some((
                    "Make executable",
                    HooksAct::MakeExecutable(hook.name.clone()),
                )),
                Problem::CrlfShebang | Problem::CrlfLines => {
                    Some(("Convert to LF", HooksAct::ToLf(hook.name.clone())))
                }
                _ => None,
            };
            if let Some((label, a)) = fix
                && ui
                    .add_enabled(!locked, egui::Button::new(label))
                    .on_disabled_hover_text("Wait for the running operation")
                    .clicked()
            {
                acts.push(act(a));
            }
        });
    }
}

/// What the commands that run hooks printed in this session.
fn output_card(ui: &mut Ui, runs: &[Run]) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        "Output of the last operations",
        Some(
            "What git and its hooks printed while committing, merging, pushing, checking out… \
             in this repository since gitcraft started. Kept in memory only.",
        ),
        |ui| {
            if runs.is_empty() {
                faint(
                    ui,
                    "Nothing yet: no operation that runs hooks printed anything.",
                );
                return;
            }
            for (i, run) in runs.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.label(code(&format!("git {}", run.command)).color(t.text));
                    if run.ok {
                        widgets::chip(ui, "done", t.success);
                    } else {
                        widgets::chip(ui, "failed", t.danger);
                    }
                    ui.label(RichText::new(ago(run.when)).color(t.text_faint));
                });
                let lines: Vec<&str> = run.output.lines().collect();
                let shown = &lines[lines.len().saturating_sub(OUTPUT_LINES)..];
                egui::Frame::new()
                    .fill(t.field)
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if shown.len() < lines.len() {
                            faint(ui, format!("… {} lines above", lines.len() - shown.len()));
                        }
                        ui.add(egui::Label::new(code(&shown.join("\n")).color(t.text_dim)).wrap());
                    });
            }
        },
    );
}

/// How long ago `when` was, in short.
fn ago(when: std::time::SystemTime) -> String {
    let secs = when.elapsed().map_or(0, |d| d.as_secs());
    match secs {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}
