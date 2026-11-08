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
    faint(ui, crate::i18n::text("repo-workflows-doctor-ui-11781882"));
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
                Some(crate::i18n::text("repo-workflows-doctor-ui-9b3b999d"))
            } else if locked {
                Some(crate::i18n::text("repo-workflows-doctor-ui-b5c82673"))
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
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-folder-card-548df737"),
        None,
        |ui| {
            ui.label(code(&found.folder).color(t.text));
            match found.hooks_path.as_deref() {
                Some("") => {}
                Some(path) => dim(
                    ui,
                    crate::i18n::message(
                        "repo-workflows-doctor-folder-card-05f75db2",
                        &[("path", (path).clone().into())],
                    ),
                ),
                None => dim(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-folder-card-34845b15"),
                ),
            }
            if found.disabled {
                warn(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-folder-card-8fa06071"),
                );
            } else if found.missing {
                warn(
                    ui,
                    &crate::i18n::message(
                        "repo-workflows-doctor-missing-hooks",
                        &[(
                            "custom",
                            (if found.hooks_path.is_some() {
                                "yes"
                            } else {
                                "no"
                            })
                            .into(),
                        )],
                    ),
                );
            }
            match found.samples {
                0 => {}
                1 => faint(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-folder-card-63c1e34d"),
                ),
                n => faint(
                    ui,
                    crate::i18n::message(
                        "repo-workflows-doctor-folder-card-605924cc",
                        &[("n", (n).clone().into())],
                    ),
                ),
            }
            for m in &found.managers {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    widgets::chip(ui, m.label(), t.accent);
                    dim(
                        ui,
                        crate::i18n::message(
                            "repo-workflows-doctor-folder-card-5cc7fab9",
                            &[("m-advice-1", (m.advice()).clone().into())],
                        ),
                    );
                });
            }
        },
    );
}

fn warn(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).color(t.warning)).wrap());
}

/// The active hooks, what runs each, and their problems; the files that never run apart.
fn hooks_card(ui: &mut Ui, found: &Hooks, locked: Option<&str>, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let title = match found.active.len() {
        0 => crate::i18n::text("repo-workflows-doctor-hooks-card-84fbcccd").to_owned(),
        1 => crate::i18n::text("repo-workflows-doctor-hooks-card-52f0510d").to_owned(),
        n => crate::i18n::message(
            "repo-workflows-doctor-hooks-card-d028a324",
            &[("n", (n).clone().into())],
        ),
    };
    card(ui, &title, None, |ui| {
        if found.active.is_empty() {
            faint(
                ui,
                crate::i18n::text("repo-workflows-doctor-hooks-card-6c856ad1"),
            );
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
                RichText::new(crate::i18n::text(
                    "repo-workflows-doctor-hooks-card-0dfb736a",
                ))
                .font(crate::theme::semibold(12.5))
                .color(t.text_dim),
            );
            faint(
                ui,
                crate::i18n::text("repo-workflows-doctor-hooks-card-69d27675"),
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
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-doctor-hook-row-433c4667",
                    &[("on", (on).clone().into())],
                ))
                .color(t.text_dim),
            );
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
                Problem::NotExecutable => Some((
                    crate::i18n::text("repo-workflows-doctor-hook-row-96c0aa61"),
                    HooksAct::MakeExecutable(target.clone()),
                )),
                Problem::CrlfShebang | Problem::CrlfLines => Some((
                    crate::i18n::text("repo-workflows-doctor-hook-row-432f4eba"),
                    HooksAct::ToLf(target.clone()),
                )),
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
        crate::i18n::text("repo-workflows-doctor-output-card-01623799"),
        Some(crate::i18n::text(
            "repo-workflows-doctor-output-card-ae5bfc74",
        )),
        |ui| {
            if runs.is_empty() {
                faint(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-output-card-50f9b973"),
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
                            faint(
                                ui,
                                crate::i18n::message(
                                    "repo-workflows-doctor-output-card-109b47bc",
                                    &[(
                                        "lines-len-shown-len-1",
                                        (lines.len() - shown.len()).clone().into(),
                                    )],
                                ),
                            );
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
        0..=59 => crate::i18n::text("repo-workflows-doctor-ago-7ddb44d8").to_owned(),
        60..=3599 => crate::i18n::message(
            "repo-workflows-doctor-ago-a0c430f4",
            &[("secs-60-1", (secs / 60).clone().into())],
        ),
        _ => crate::i18n::message(
            "repo-workflows-doctor-ago-4603eba2",
            &[("secs-3600-1", (secs / 3600).clone().into())],
        ),
    }
}
