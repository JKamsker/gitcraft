//! The Activity panel: the tab's operations, newest first, each unfolding into its git
//! commands, how each ended and all it printed, to read or copy.

use egui::{RichText, Ui};

use super::{Entry, Outcome, command_line, exit_text, outcome_text, seconds};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::{self, Tokens};

/// How tall one command's output gets before it scrolls.
const OUTPUT_HEIGHT: f32 = 220.0;

impl RepoTab {
    /// The panel's contents: a header (count, Clear, close), then the operations.
    pub(in crate::repo) fn activity_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Activity").font(theme::semibold(12.5)));
            ui.label(
                RichText::new(format!("{} operations, newest first", self.activity.len()))
                    .color(t.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::small_button(ui, "x", "Close the Activity panel").clicked() {
                    acts.push(Action::ToggleActivity);
                }
                if ui
                    .add_enabled(self.activity.len() > 0, egui::Button::new("Clear"))
                    .on_hover_text("Forget the operations listed here")
                    .clicked()
                {
                    self.activity.clear();
                }
            });
        });
        ui.separator();
        let reveal = self.activity.reveal.take();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.activity.len() == 0 && self.busy_op.is_none() {
                    ui.label(
                        RichText::new(
                            "Nothing yet: what this tab runs (fetch, commit, checkout…) is listed \
                             here with its git commands and all they printed.",
                        )
                        .color(t.text_dim),
                    );
                }
                self.running_entry(ui, &t);
                for entry in self.activity.entries().rev() {
                    entry_ui(ui, entry, reveal == Some(entry.id), &t);
                }
            });
    }

    /// The operation running, with the commands it ran so far.
    fn running_entry(&self, ui: &mut Ui, t: &Tokens) {
        let (Some(op), Some(running)) = (&self.busy_op, &self.running) else {
            return;
        };
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(RichText::new(op).font(theme::medium(12.5)));
            if let Some(p) = running.control.progress() {
                ui.label(RichText::new(p.label()).color(t.text_dim));
            }
        });
        for step in running.control.steps() {
            ui.label(
                RichText::new(format!(
                    "$ {}  · {}",
                    command_line(&step.args),
                    exit_text(step.code)
                ))
                .font(theme::mono(11.5))
                .color(t.text_dim),
            );
        }
        ui.add_space(4.0);
    }
}

/// One operation: a line saying what, how it ended and how long it took, unfolding into its
/// commands.
fn entry_ui(ui: &mut Ui, entry: &Entry, reveal: bool, t: &Tokens) {
    let (icon, color) = match entry.outcome {
        Outcome::Done => ("check", t.success),
        Outcome::Failed(_) => ("circle-alert", t.danger),
        Outcome::Cancelled => ("x", t.text_dim),
    };
    let id = ui.id().with(("activity-entry", entry.id));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
    if reveal {
        state.set_open(true);
    }
    let header = state.show_header(ui, |ui| {
        icons::inline(ui, icon, 13.0, color);
        ui.label(RichText::new(&entry.label).font(theme::medium(12.5)));
        let mut when = format!(
            "{} · {} · {} ago",
            outcome_text(&entry.outcome),
            seconds(entry.took),
            ago(entry.at.elapsed())
        );
        if entry.background {
            when.push_str(" · in the background");
        }
        ui.label(RichText::new(when).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::small_button(ui, "copy", "Copy the commands and all they printed").clicked() {
                ui.ctx().copy_text(entry.as_text());
            }
        });
    });
    let (_, header_resp, _) = header.body(|ui| {
        if let Outcome::Failed(why) = &entry.outcome {
            ui.label(RichText::new(why).color(t.danger));
        }
        if entry.steps.is_empty() {
            ui.label(RichText::new("No git command ran.").color(t.text_dim));
        }
        for (i, step) in entry.steps.iter().enumerate() {
            ui.label(
                RichText::new(format!(
                    "$ {}  · {} · {}",
                    command_line(&step.args),
                    exit_text(step.code),
                    seconds(step.took)
                ))
                .font(theme::mono(11.5)),
            );
            if !step.output.is_empty() {
                egui::ScrollArea::vertical()
                    .id_salt(("activity-output", entry.id, i))
                    .max_height(OUTPUT_HEIGHT)
                    .show(ui, |ui| {
                        // Read-only, but selectable and copyable.
                        let mut text = step.output.as_str();
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .font(theme::mono(11.5))
                                .desired_width(f32::INFINITY)
                                .desired_rows(1),
                        );
                    });
            }
        }
    });
    if reveal {
        header_resp.response.scroll_to_me(Some(egui::Align::TOP));
    }
}

/// "12 s", "3 min", "2 h".
fn ago(d: std::time::Duration) -> String {
    match d.as_secs() {
        s @ 0..60 => format!("{s} s"),
        s @ 60..3600 => format!("{} min", s / 60),
        s => format!("{} h", s / 3600),
    }
}
