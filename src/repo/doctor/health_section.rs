//! Health and maintenance: how big the repository is, how long reading it takes, what would
//! make it faster (each with one click), and what the history limit means for a huge one.

use egui::{RichText, Ui};

use super::health::{HealthState, Loading};
use super::page::{card, code, dim, faint, reading};
use super::{Confirm, DoctorAct};
use crate::git::doctor::health::{self, Health, State, Suggestion};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

pub(super) fn ui(
    ui: &mut Ui,
    state: &HealthState,
    loading: &Loading,
    locked: bool,
    acts: &mut Vec<Action>,
) {
    faint(ui, crate::i18n::text("repo-workflows-doctor-ui-c7deeea1"));
    let Some(h) = reading(ui, &state.found) else {
        return;
    };
    size_card(ui, h);
    speed_card(ui, h, loading);
    suggestions_card(ui, h, locked, acts);
    history_card(ui, h, loading);
}

/// KiB as it reads best.
pub(super) fn size(kib: u64) -> String {
    match kib {
        0..1024 => crate::i18n::message(
            "repo-workflows-doctor-size-95886483",
            &[("kib", (kib).into())],
        ),
        1024..1_048_576 => crate::i18n::message(
            "repo-workflows-doctor-size-8333f24e",
            &[("kib-as-f64-1024-0-1", (kib as f64 / 1024.0).into())],
        ),
        _ => crate::i18n::message(
            "repo-workflows-doctor-size-9d2b746f",
            &[(
                "kib-as-f64-1-048-576-0-1",
                (kib as f64 / 1_048_576.0).into(),
            )],
        ),
    }
}

fn size_card(ui: &mut Ui, h: &Health) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-size-card-1af85190"),
        Some("git count-objects -v, git rev-list --count --all"),
        |ui| {
            let o = &h.objects;
            egui::Grid::new("doctor-size")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    let row = |ui: &mut Ui, what: &str, value: String| {
                        ui.label(RichText::new(what).color(t.text_dim));
                        ui.label(code(&value).color(t.text));
                        ui.end_row();
                    };
                    if let Some(n) = h.commits {
                        row(
                            ui,
                            crate::i18n::text("repo-workflows-doctor-size-card-d09648c6"),
                            crate::i18n::number(n),
                        );
                    }
                    row(
                        ui,
                        crate::i18n::text("repo-workflows-doctor-size-card-35195164"),
                        crate::i18n::message(
                            "repo-workflows-doctor-size-card-1f2fe3f1",
                            &[
                                ("o-packed-1", (o.packed).into()),
                                ("o-packs-2", (o.packs).into()),
                                ("size-o-pack-kib-3", (size(o.pack_kib)).clone().into()),
                            ],
                        ),
                    );
                    row(
                        ui,
                        crate::i18n::text("repo-workflows-doctor-size-card-8757152b"),
                        format!("{}, {}", o.loose, size(o.loose_kib)),
                    );
                    if o.prune_packable > 0 {
                        row(
                            ui,
                            crate::i18n::text("repo-workflows-doctor-size-card-b83a7e1d"),
                            o.prune_packable.to_string(),
                        );
                    }
                });
            if o.garbage > 0 {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-doctor-size-card-52c9d559",
                        &[
                            ("o-garbage-1", (o.garbage).into()),
                            ("size-o-garbage-kib-2", (size(o.garbage_kib)).clone().into()),
                        ],
                    ))
                    .color(t.warning),
                );
            }
        },
    );
}

fn speed_card(ui: &mut Ui, h: &Health, loading: &Loading) {
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-speed-card-2154776b"),
        None,
        |ui| {
            if let Some(took) = h.status_took {
                dim(
                    ui,
                    crate::i18n::message(
                        "repo-workflows-doctor-speed-card-0ad7db50",
                        &[("took-as-millis-1", (took.as_millis()).into())],
                    ),
                );
            }
            if loading.times.is_empty() {
                faint(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-speed-card-ffe6b51a"),
                );
            } else {
                let times: Vec<String> = loading
                    .times
                    .iter()
                    .map(|d| {
                        crate::i18n::message(
                            "repo-workflows-doctor-speed-card-8636e9ce",
                            &[("d-as-millis-1", (d.as_millis()).into())],
                        )
                    })
                    .collect();
                dim(
                    ui,
                    crate::i18n::message(
                        "repo-workflows-doctor-speed-card-aebc38ca",
                        &[("times-join-1", (times.join(", ")).clone().into())],
                    ),
                );
            }
        },
    );
}

fn suggestions_card(ui: &mut Ui, h: &Health, locked: bool, acts: &mut Vec<Action>) {
    let list = health::suggestions(h);
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-suggestions-card-e930084c"),
        None,
        |ui| {
            for (i, s) in list.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                suggestion(ui, s, locked, acts);
            }
        },
    );
}

fn suggestion(ui: &mut Ui, s: &Suggestion, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(s.display_title())
                .font(crate::theme::semibold(13.0))
                .color(t.text),
        );
        match s.state {
            State::On => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-suggestion-b8d31e85"),
                t.success,
            ),
            // A one-off action rather than something left on.
            State::Off if s.title == "Pack the objects" => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-suggestion-8ad70375"),
                t.warning,
            ),
            State::Off => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-suggestion-b4dc66dd"),
                t.text_faint,
            ),
            State::Unsupported => widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-suggestion-2b7f0cfe"),
                t.text_faint,
            ),
        };
    });
    dim(ui, &s.why);
    let commands: Vec<String> = s
        .ops
        .iter()
        .map(|op| format!("git {}", op.join(" ")))
        .collect();
    match s.state {
        State::On => {}
        State::Unsupported if s.title == "File system monitor" => faint(
            ui,
            crate::i18n::text("repo-workflows-doctor-suggestion-a96e0fc8"),
        ),
        State::Unsupported => {}
        State::Off => {
            ui.horizontal_wrapped(|ui| {
                let turn_on = ui
                    .add_enabled(
                        !locked,
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-doctor-suggestion-5a1f096a",
                        )),
                    )
                    .on_hover_text(commands.join("\n"))
                    .on_disabled_hover_text(crate::i18n::text(
                        "repo-workflows-doctor-suggestion-b5c82673",
                    ));
                if turn_on.clicked() {
                    acts.push(turn_on_action(s, &commands));
                }
                ui.label(code(&commands.join(" · ")).color(t.text_faint));
            });
        }
    }
}

/// Runs the suggestion's commands; scheduling maintenance (which reaches past the repository:
/// your global config and the system's scheduler) is asked first.
fn turn_on_action(s: &Suggestion, commands: &[String]) -> Action {
    let label = format!("Turn on: {}", s.title.to_lowercase());
    if s.title == "Scheduled maintenance" {
        return Action::Doctor(DoctorAct::Confirm(Confirm {
            title: "Schedule maintenance?".to_owned(),
            body: format!(
                "{} adds this repository to maintenance.repo in your own global git config, sets \
                 maintenance.auto=false and maintenance.strategy=incremental in its config, and \
                 schedules git to run in the background: every hour it fetches from the \
                 remotes into refs/prefetch. The scheduled runs read your own global config, \
                 not a gitcraft profile's. git maintenance unregister undoes it for this \
                 repository (and git config --unset maintenance.auto brings automatic gc \
                 back); git maintenance stop removes the schedule for every repository.",
                commands.join(", ")
            ),
            button: "Schedule".to_owned(),
            label,
            ops: s.ops.clone(),
            presentation: None,
} .with_presentation(super::ConfirmKind::Maintenance { commands: commands.join(", ") })));
    }
    Action::Doctor(DoctorAct::Run {
        label,
        ops: s.ops.clone(),
    })
}

/// What the history limit means here.
fn history_card(ui: &mut Ui, h: &Health, loading: &Loading) {
    let t = Tokens::get(ui.ctx());
    let max = crate::repo::MAX_HISTORY;
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-history-card-06c896b6"),
        None,
        |ui| {
            dim(
                ui,
                crate::i18n::message(
                    "repo-workflows-doctor-history-loaded",
                    &[
                        ("page", (crate::git::LOG_PAGE).into()),
                        ("max", (max).into()),
                        ("loaded", (loading.loaded).into()),
                        ("more", (if loading.more { "yes" } else { "no" }).into()),
                    ],
                ),
            );
            if h.commits.is_some_and(|n| n > max as u64) {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-doctor-history-card-cd12e41d",
                        &[(
                            "group-h-commits-unwrap-or-defaul-1",
                            h.commits.unwrap_or_default().into(),
                        )],
                    ))
                    .color(t.warning),
                );
            }
            if !h.commit_graph {
                faint(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-history-card-5a389005"),
                );
            }
        },
    );
}

/// `n` with thin groups of three digits (100 000).
#[cfg(test)]
pub(super) fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
#[path = "health_section/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "health_section/fluent_confirmation_tests.rs"]
mod fluent_confirmation_tests;

#[cfg(test)]
#[path = "health_section/fluent_r1_maintenance_cache_tests.rs"]
mod fluent_r1_maintenance_cache_tests;
