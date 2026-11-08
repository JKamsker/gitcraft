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
            &[("kib", (kib).clone().into())],
        ),
        1024..1_048_576 => crate::i18n::message(
            "repo-workflows-doctor-size-8333f24e",
            &[("kib-as-f64-1024-0-1", (kib as f64 / 1024.0).clone().into())],
        ),
        _ => crate::i18n::message(
            "repo-workflows-doctor-size-9d2b746f",
            &[(
                "kib-as-f64-1-048-576-0-1",
                (kib as f64 / 1_048_576.0).clone().into(),
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
                            n.to_string(),
                        );
                    }
                    row(
                        ui,
                        crate::i18n::text("repo-workflows-doctor-size-card-35195164"),
                        crate::i18n::message(
                            "repo-workflows-doctor-size-card-1f2fe3f1",
                            &[
                                ("o-packed-1", (o.packed).clone().into()),
                                ("o-packs-2", (o.packs).clone().into()),
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
                            ("o-garbage-1", (o.garbage).clone().into()),
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
                        &[("took-as-millis-1", (took.as_millis()).clone().into())],
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
                            &[("d-as-millis-1", (d.as_millis()).clone().into())],
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
            State::On => widgets::chip(ui, "on", t.success),
            // A one-off action rather than something left on.
            State::Off if s.title == "Pack the objects" => {
                widgets::chip(ui, "suggested", t.warning)
            }
            State::Off => widgets::chip(ui, "off", t.text_faint),
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
            title: crate::i18n::text("repo-workflows-doctor-turn-on-action-3c97132f").to_owned(),
            body: crate::i18n::message(
                "repo-workflows-doctor-turn-on-action-7cde0a5e",
                &[("commands-join-1", (commands.join(", ")).clone().into())],
            ),
            button: crate::i18n::text("repo-workflows-doctor-turn-on-action-f4830a1d").to_owned(),
            label,
            ops: s.ops.clone(),
        }));
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
                            (group(h.commits.unwrap_or_default())).clone().into(),
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
mod tests {
    use super::*;

    #[test]
    fn numbers_read_well() {
        assert_eq!(group(100_000), "100 000");
        assert_eq!(group(999), "999");
        assert_eq!(group(1_234_567), "1 234 567");
        assert_eq!(group(0), "0");
        assert_eq!(size(512), "512 KiB");
        assert_eq!(size(2048), "2.0 MiB");
        assert_eq!(size(3 * 1_048_576), "3.00 GiB");
    }
}

#[cfg(test)]
mod fluent_confirmation_tests {
    use super::*;

    #[test]
    fn fluent_partition_doctor_actual_selection_keeps_confirmation_and_argv_in_both_languages()
    -> Result<(), String> {
        // Supplied state only: health::suggestions and turn_on_action do no discovery or I/O.
        let supplied = Health {
            objects: health::Objects {
                loose: 5000,
                ..Default::default()
            },
            fsmonitor: Some(State::Off),
            ..Default::default()
        };
        let titles = [
            "Commit-graph",
            "Scheduled maintenance",
            "File system monitor",
            "Untracked cache",
            "Pack the objects",
        ];
        let german = [
            "Commit-Graph",
            "Geplante Wartung",
            "Dateisystemmonitor",
            "Cache unversionierter Dateien",
            "Objekte packen",
        ];
        let expected_ops = [
            vec![
                vec!["commit-graph", "write", "--reachable", "--changed-paths"],
                vec!["config", "--local", "fetch.writeCommitGraph", "true"],
            ],
            vec![vec!["maintenance", "start"]],
            vec![vec!["config", "--local", "core.fsmonitor", "true"]],
            vec![
                vec!["update-index", "--test-untracked-cache"],
                vec!["config", "--local", "core.untrackedCache", "true"],
                vec!["update-index", "--untracked-cache"],
            ],
            vec![vec!["gc", "--quiet"]],
        ];
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let suggestions = health::suggestions(&supplied);
            assert_eq!(suggestions.len(), titles.len());
            for (index, suggestion) in suggestions.iter().enumerate() {
                assert_eq!(suggestion.title, titles[index]);
                assert_eq!(
                    suggestion.display_title(),
                    if language == "de" {
                        german[index]
                    } else {
                        titles[index]
                    }
                );
                assert_eq!(suggestion.state, State::Off);
                assert_eq!(suggestion.ops, expected_ops[index]);
                let commands: Vec<_> = suggestion
                    .ops
                    .iter()
                    .map(|op| format!("git {}", op.join(" ")))
                    .collect();
                // This is the same selection function called by the actual Turn on button.
                let action = turn_on_action(suggestion, &commands);
                let expected_label = format!("Turn on: {}", titles[index].to_lowercase());
                match action {
                    Action::Doctor(DoctorAct::Confirm(confirmation)) if index == 1 => {
                        assert_eq!(
                            confirmation.title,
                            if language == "de" {
                                "Wartung planen?"
                            } else {
                                "Schedule maintenance?"
                            }
                        );
                        assert_eq!(
                            confirmation.button,
                            if language == "de" {
                                "Planen"
                            } else {
                                "Schedule"
                            }
                        );
                        assert!(confirmation.body.starts_with("git maintenance start "));
                        assert!(confirmation.body.contains(if language == "de" {
                            "trägt dieses Repository"
                        } else {
                            "adds this repository"
                        }));
                        for literal in [
                            "maintenance.repo",
                            "maintenance.auto=false",
                            "maintenance.strategy=incremental",
                            "refs/prefetch",
                            "git maintenance unregister",
                            "git maintenance stop",
                        ] {
                            assert!(
                                confirmation.body.contains(literal),
                                "missing literal {literal}"
                            );
                        }
                        assert_eq!(confirmation.label, expected_label);
                        assert_eq!(confirmation.ops, expected_ops[index]);
                    }
                    Action::Doctor(DoctorAct::Run { label, ops }) if index != 1 => {
                        assert_eq!(label, expected_label);
                        assert_eq!(ops, expected_ops[index]);
                    }
                    _ => {
                        return Err(format!(
                            "incorrect confirmation/run selection for {language}: {}",
                            suggestion.title
                        ));
                    }
                }
            }
            let unsupported = health::suggestions(&Health {
                fsmonitor: Some(State::Unsupported),
                ..supplied.clone()
            });
            assert_eq!(unsupported[2].title, "File system monitor");
            assert_eq!(unsupported[2].state, State::Unsupported);
            assert_eq!(unsupported[4].title, "Pack the objects");
            let opaque = Suggestion {
                title: "Geplante Wartung / external",
                why: String::new(),
                state: State::Off,
                ops: vec![vec!["opaque-argv".into()]],
            };
            assert_eq!(opaque.display_title(), opaque.title);
            match turn_on_action(&opaque, &["git opaque-argv".into()]) {
                Action::Doctor(DoctorAct::Run { label, ops }) => {
                    assert_eq!(label, "Turn on: geplante wartung / external");
                    assert_eq!(ops, opaque.ops);
                }
                _ => return Err("unknown title changed confirmation identity".into()),
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}
