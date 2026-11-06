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
            ui.add(egui::Label::new(RichText::new(done).color(t.success)).wrap());
        }
        Some(Err(e)) => {
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
        }
        None => {}
    }
    if let Some(found) = reading(ui, &state.found) {
        folder_card(ui, found);
        if !found.disabled {
            let why = if state.fixing {
                Some("A fix is being made")
            } else if locked {
                Some("Wait for the running operation")
            } else {
                None
            };
            hooks_card(ui, found, why, acts);
        }
    }
    output_card(ui, &state.runs);
}

/// The folder git runs hooks from, and the tools that manage them.
fn folder_card(ui: &mut Ui, found: &Hooks) {
    let t = Tokens::get(ui.ctx());
    card(ui, "Hooks folder", None, |ui| {
        ui.label(code(&found.folder).color(t.text));
        match found.hooks_path.as_deref() {
            Some("") => {}
            Some(path) => dim(
                ui,
                format!("Set by core.hooksPath ({path}), instead of .git/hooks."),
            ),
            None => dim(
                ui,
                "The repository's own (.git/hooks): no core.hooksPath is set.",
            ),
        }
        if found.disabled {
            warn(
                ui,
                "core.hooksPath is set to nothing: git runs no hook at all. Unset it (Which \
                 configuration wins) to run the hooks again.",
            );
        } else if found.missing {
            let next = if found.hooks_path.is_some() {
                " Whatever set core.hooksPath (a hook manager?) isn't installed: run its install \
                 step, or unset core.hooksPath."
            } else {
                ""
            };
            warn(
                ui,
                &format!("This folder doesn't exist (or isn't a folder): no hook runs.{next}"),
            );
        }
        match found.samples {
            0 => {}
            1 => faint(ui, "1 .sample file: an example git ships, never run."),
            n => faint(
                ui,
                format!("{n} .sample files: examples git ships, never run."),
            ),
        }
        for m in &found.managers {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, m.label(), t.accent);
                dim(ui, format!("manages the hooks here: {}", m.advice()));
            });
        }
    });
}

fn warn(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).color(t.warning)).wrap());
}

/// The active hooks, what runs each, and their problems; the files that never run apart.
fn hooks_card(ui: &mut Ui, found: &Hooks, locked: Option<&str>, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = match found.active.len() {
        0 => "No active hooks".to_owned(),
        1 => "1 hook".to_owned(),
        n => format!("{n} hooks"),
    };
    card(ui, &title, None, |ui| {
        if found.active.is_empty() {
            faint(ui, "No hook runs: every operation goes ahead without one.");
        }
        for (i, hook) in found.active.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            hook_row(ui, hook, locked, acts);
        }
        if !found.others.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Other files (never run)")
                    .font(crate::theme::semibold(12.5))
                    .color(t.text_dim),
            );
            faint(
                ui,
                "Git runs no hook of these names: a typo, or a copy kept aside.",
            );
            ui.horizontal_wrapped(|ui| {
                for other in &found.others {
                    ui.label(code(&other.name).color(t.text_faint));
                }
            });
        }
    });
}

fn hook_row(ui: &mut Ui, hook: &Hook, locked: Option<&str>, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    // The path it is fixed by: a husky script's from the top, else its name in the folder.
    let target = hook.file.clone().unwrap_or_else(|| hook.name.clone());
    ui.horizontal_wrapped(|ui| {
        ui.label(code(&target).color(t.text));
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
            ui.add(egui::Label::new(RichText::new(problem.describe()).color(t.warning)).wrap());
            let fix = match problem {
                Problem::NotExecutable => {
                    Some(("Make executable", HooksAct::MakeExecutable(target.clone())))
                }
                Problem::CrlfShebang | Problem::CrlfLines => {
                    Some(("Convert to LF", HooksAct::ToLf(target.clone())))
                }
                _ => None,
            };
            if let Some((label, a)) = fix
                && ui
                    .add_enabled(locked.is_none(), egui::Button::new(label))
                    .on_disabled_hover_text(locked.unwrap_or_default())
                    .clicked()
            {
                acts.push(act(a));
            }
        });
    }
}

/// What the commands that run hooks printed.
fn output_card(ui: &mut Ui, runs: &[Run]) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        "Output of the last operations",
        Some(
            "What git and its hooks printed during the last operations gitcraft ran in this \
             repository (commit, merge, push, checkout…), the last dozen. Kept in memory only, \
             with passwords in URLs hidden.",
        ),
        |ui| {
            if runs.is_empty() {
                faint(
                    ui,
                    "Nothing yet: no operation gitcraft ran that runs hooks printed anything \
                     (what runs in the terminal isn't kept).",
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
