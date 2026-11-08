//! The strip over the graph while commits are tested: what runs on which commits, how far it
//! got, and Stop; once it ended, how it went, the first commit that failed (a click selects
//! it), and Close.

use egui::{RichText, Ui};

use super::{LaneAct, LaneRun};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::Tokens;

impl RepoTab {
    pub(in crate::repo) fn test_banner(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match &self.test_lane.run {
                    Some(run) if run.going() => {
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-test_lane-test-banner-cae7d57b",
                            ))
                            .on_hover_text(crate::i18n::text(
                                "repo-workflows-test_lane-test-banner-195bb39a",
                            ))
                            .clicked()
                        {
                            acts.push(Action::TestLane(LaneAct::Stop));
                        }
                    }
                    _ => {
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-test_lane-test-banner-7d9eb7ac",
                            ))
                            .clicked()
                        {
                            acts.push(Action::TestLane(LaneAct::Close));
                        }
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    crate::icons::inline(ui, "flask-conical", 14.0, t.accent);
                    widgets::chip(
                        ui,
                        crate::i18n::text("repo-workflows-test_lane-test-banner-e5c9d703"),
                        t.accent,
                    );
                    ui.add_space(4.0);
                    match &self.test_lane.run {
                        Some(run) => self.run_status(ui, run, acts),
                        None => {
                            let raw = self.test_lane.error.as_deref().unwrap_or_default();
                            let e = self.test_lane.display_read_error(raw);
                            ui.add(egui::Label::new(RichText::new(&e).color(t.danger)).truncate())
                                .on_hover_text(e);
                        }
                    }
                });
            });
        });
    }

    fn run_status(&self, ui: &mut Ui, run: &LaneRun, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        if let Some(Err(e)) = &run.ended {
            let said = super::display_ended_message(&run.range, &Err(e.clone())).0;
            ui.add(egui::Label::new(RichText::new(&said).color(t.danger)).truncate())
                .on_hover_text(run.ended.as_ref().map_or_else(
                    || said.clone(),
                    |result| super::display_ended_message(&run.range, result).0,
                ));
            return;
        }
        if let Some((_, commit)) = &run.first_bad {
            let label = crate::i18n::message(
                "repo-workflows-test_lane-run-status-71902fb1",
                &[("short-commit-1", (short(commit)).into())],
            );
            if ui
                .link(RichText::new(label).color(t.danger))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-test_lane-run-status-514e35e8",
                ))
                .clicked()
            {
                acts.push(Action::SelectHash(commit.clone()));
            }
            ui.add_space(6.0);
        }
        let said = progress_text(run);
        ui.add(egui::Label::new(RichText::new(&said).color(t.text_dim)).truncate())
            .on_hover_text(run.ended.as_ref().map_or_else(
                || said.clone(),
                |result| super::display_ended_message(&run.range, result).0,
            ));
    }
}

/// What the banner says of `run`: `Testing main..feature with `cargo test`: 3 of 12 done ·
/// 2 passed · 1 failed · 2 running`.
pub(super) fn progress_text(run: &LaneRun) -> String {
    let done = run.good + run.bad + run.failed;
    let mut parts = Vec::new();
    match (run.going(), run.total) {
        (true, None) => parts
            .push(crate::i18n::text("repo-workflows-test_lane-progress-text-3142ee46").to_owned()),
        (true, Some(total)) => parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-1df05863",
            &[("done", (done).into()), ("total", (total).into())],
        )),
        (false, _) => parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-6a3a9451",
            &[("done", (done).into())],
        )),
    }
    if done > 0 {
        parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-939ee5d9",
            &[("run-good-1", (run.good).into())],
        ));
    }
    if run.bad > 0 {
        parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-1516ec62",
            &[("run-bad-1", (run.bad).into())],
        ));
    }
    if run.failed > 0 {
        parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-190f5f35",
            &[("run-failed-1", (run.failed).into())],
        ));
    }
    if run.going() && !run.running.is_empty() {
        parts.push(crate::i18n::message(
            "repo-workflows-test_lane-progress-text-030be8be",
            &[("run-running-len-1", (run.running.len()).into())],
        ));
    }
    let stopped = matches!(&run.ended, Some(Ok(s)) if s.stopped);
    crate::i18n::message(
        "repo-workflows-test-lane-progress-head",
        &[
            (
                "state",
                (match (run.going(), stopped) {
                    (true, _) => "running",
                    (false, true) => "stopped",
                    _ => "done",
                })
                .into(),
            ),
            ("range", (run.range.as_str()).into()),
            ("command", (run.command.as_str()).into()),
            ("details", (parts.join(" · ")).into()),
        ],
    )
}
