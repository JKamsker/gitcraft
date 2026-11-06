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
    faint(
        ui,
        "How big the repository is, how long reading it takes, and what would make git faster \
         here. Every speed-up is git's own and works for git everywhere, not only in gitcraft.",
    );
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
        0..1024 => format!("{kib} KiB"),
        1024..1_048_576 => format!("{:.1} MiB", kib as f64 / 1024.0),
        _ => format!("{:.2} GiB", kib as f64 / 1_048_576.0),
    }
}

fn size_card(ui: &mut Ui, h: &Health) {
    let t = Tokens::get(ui.ctx());
    card(
        ui,
        "Size",
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
                        row(ui, "Commits", n.to_string());
                    }
                    row(
                        ui,
                        "Packed objects",
                        format!("{} in {} packs, {}", o.packed, o.packs, size(o.pack_kib)),
                    );
                    row(
                        ui,
                        "Loose objects",
                        format!("{}, {}", o.loose, size(o.loose_kib)),
                    );
                    if o.prune_packable > 0 {
                        row(ui, "Loose and also packed", o.prune_packable.to_string());
                    }
                });
            if o.garbage > 0 {
                ui.label(
                RichText::new(format!(
                    "{} files in the objects folder aren't objects ({}): left by an interrupted \
                     operation; git gc reports them.",
                    o.garbage,
                    size(o.garbage_kib)
                ))
                .color(t.warning),
            );
            }
        },
    );
}

fn speed_card(ui: &mut Ui, h: &Health, loading: &Loading) {
    card(ui, "How long reading it takes", None, |ui| {
        if let Some(took) = h.status_took {
            dim(
                ui,
                format!(
                    "git status: {} ms (gitcraft runs it on every change it sees).",
                    took.as_millis()
                ),
            );
        }
        if loading.times.is_empty() {
            faint(ui, "No reload of the tab timed yet.");
        } else {
            let times: Vec<String> = loading
                .times
                .iter()
                .map(|d| format!("{} ms", d.as_millis()))
                .collect();
            dim(
                ui,
                format!(
                    "The tab's last reloads (branches, history, changes, all at once): {}.",
                    times.join(", ")
                ),
            );
        }
    });
}

fn suggestions_card(ui: &mut Ui, h: &Health, locked: bool, acts: &mut Vec<Action>) {
    let list = health::suggestions(h);
    card(ui, "What would make it faster", None, |ui| {
        for (i, s) in list.iter().enumerate() {
            if i > 0 {
                ui.separator();
            }
            suggestion(ui, s, locked, acts);
        }
    });
}

fn suggestion(ui: &mut Ui, s: &Suggestion, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(s.title)
                .font(crate::theme::semibold(13.0))
                .color(t.text),
        );
        match s.state {
            State::On => widgets::chip(ui, "on", t.success),
            State::Off => widgets::chip(ui, "off", t.text_faint),
            State::Unsupported => widgets::chip(ui, "not available here", t.text_faint),
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
        State::Unsupported => faint(
            ui,
            "git's own monitor exists on Windows and macOS only; on Linux a Watchman hook can \
             stand in (core.fsmonitor naming it).",
        ),
        State::Off => {
            ui.horizontal_wrapped(|ui| {
                let turn_on = ui
                    .add_enabled(!locked, egui::Button::new("Turn on"))
                    .on_hover_text(commands.join("\n"))
                    .on_disabled_hover_text("Wait for the running operation");
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
                "{} adds this repository to maintenance.repo in your global git config and \
                 schedules git to run in the background every hour (git maintenance stop \
                 undoes it).",
                commands.join(", ")
            ),
            button: "Schedule".to_owned(),
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
    card(ui, "History in the graph", None, |ui| {
        dim(
            ui,
            format!(
                "The graph loads {} commits at a time as you scroll, and at most {} in all; \
                 search loads more as it needs them, up to that limit too. {} loaded now{}.",
                crate::git::LOG_PAGE,
                group(max as u64),
                group(loading.loaded as u64),
                if loading.more { ", more to load" } else { "" }
            ),
        );
        if h.commits.is_some_and(|n| n > max as u64) {
            ui.label(
                RichText::new(format!(
                    "This repository has {} commits, more than the graph shows: the oldest \
                     ones are only reachable from the terminal (git log) or a file's history. \
                     Hiding branches or soloing some narrows the graph to theirs.",
                    group(h.commits.unwrap_or_default())
                ))
                .color(t.warning),
            );
        }
        if !h.commit_graph {
            faint(
                ui,
                "Each reload reads the history again and rebuilds the graph: with a \
                 commit-graph, git reads it several times faster.",
            );
        }
    });
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
