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
                            .button("Stop")
                            .on_hover_text(
                                "Stops the tests: those that finished keep their result, those \
                                 part way through are dropped",
                            )
                            .clicked()
                        {
                            acts.push(Action::TestLane(LaneAct::Stop));
                        }
                    }
                    _ => {
                        if ui.button("Close").clicked() {
                            acts.push(Action::TestLane(LaneAct::Close));
                        }
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    crate::icons::inline(ui, "flask-conical", 14.0, t.accent);
                    widgets::chip(ui, "Tests", t.accent);
                    ui.add_space(4.0);
                    match &self.test_lane.run {
                        Some(run) => self.run_status(ui, run, acts),
                        None => {
                            let e = self.test_lane.error.as_deref().unwrap_or_default();
                            ui.add(egui::Label::new(RichText::new(e).color(t.danger)).truncate())
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
            let said = format!("Couldn't test {}: {e}", run.range);
            ui.add(egui::Label::new(RichText::new(&said).color(t.danger)).truncate())
                .on_hover_text(said);
            return;
        }
        if let Some((_, commit)) = &run.first_bad {
            let label = format!("First failure: {}", short(commit));
            if ui
                .link(RichText::new(label).color(t.danger))
                .on_hover_text("Select the oldest commit that failed")
                .clicked()
            {
                acts.push(Action::SelectHash(commit.clone()));
            }
            ui.add_space(6.0);
        }
        let said = progress_text(run);
        ui.add(egui::Label::new(RichText::new(&said).color(t.text_dim)).truncate())
            .on_hover_text(said);
    }
}

/// What the banner says of `run`: `Testing main..feature with `cargo test`: 3 of 12 done ·
/// 2 passed · 1 failed · 2 running`.
pub(super) fn progress_text(run: &LaneRun) -> String {
    let done = run.good + run.bad + run.failed;
    let mut parts = Vec::new();
    match (run.going(), run.total) {
        (true, None) => parts.push("listing the commits…".to_owned()),
        (true, Some(total)) => parts.push(format!("{done} of {total} done")),
        (false, _) => parts.push(format!("{done} done")),
    }
    if done > 0 {
        parts.push(format!("{} passed", run.good));
    }
    if run.bad > 0 {
        parts.push(format!("{} failed", run.bad));
    }
    if run.failed > 0 {
        parts.push(format!("{} couldn't be tested", run.failed));
    }
    if run.going() && !run.running.is_empty() {
        parts.push(format!("{} running", run.running.len()));
    }
    let stopped = matches!(&run.ended, Some(Ok(s)) if s.stopped);
    let verb = match (run.going(), stopped) {
        (true, _) => "Testing",
        (false, true) => "Stopped testing",
        (false, false) => "Tested",
    };
    format!(
        "{verb} {} with `{}`: {}",
        run.range,
        run.command,
        parts.join(" · ")
    )
}
