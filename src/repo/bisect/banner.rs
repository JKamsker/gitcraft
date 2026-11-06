//! The banner over the graph while a bisect goes on: where it is (the commit being tested and
//! about how many steps are left, or what it found), Good / Bad / Skip, a command to run, Stop;
//! and under it the output of `git bisect run`.

use egui::{RichText, Ui};

use super::{BisectAct, RunLog};
use crate::git::{Bisect, Mark, Outcome};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

/// The most commits named when only skipped ones are left.
const MAX_NAMED: usize = 8;

/// From this width on, the banner's buttons share a row with what it says.
const ONE_ROW_W: f32 = 980.0;

/// The height of the run's output.
const LOG_H: f32 = 180.0;

impl RepoTab {
    pub(in crate::repo) fn bisect_banner(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if let Some(bisect) = self.bisect.state() {
            if ui.available_width() >= ONE_ROW_W {
                // The buttons first, on the right; what it says in the room left, cut to fit.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.bisect_buttons(ui, bisect, acts);
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        self.bisect_status(ui, bisect, acts);
                    });
                });
            } else {
                ui.horizontal(|ui| self.bisect_status(ui, bisect, acts));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.bisect_buttons(ui, bisect, acts);
                    });
                });
            }
        } else if let Some(e) = &self.bisect.error {
            let t = Tokens::get(ui.ctx());
            ui.colored_label(t.danger, format!("Couldn't read the bisect: {e}"));
        }
        if let Some(run) = &self.bisect.run {
            run_log(ui, run, acts);
        }
    }

    /// The banner's name, then where the bisect is, cut to the room there is.
    fn bisect_status(&self, ui: &mut Ui, bisect: &Bisect, acts: &mut Vec<Action>) {
        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
        let t = Tokens::get(ui.ctx());
        crate::icons::inline(ui, "search", 14.0, t.accent);
        widgets::chip(ui, "Bisecting", t.accent);
        ui.add_space(4.0);
        self.where_it_is(ui, bisect, acts);
    }

    /// The commit being tested and the steps left, what is missing to start, or the outcome.
    fn where_it_is(&self, ui: &mut Ui, bisect: &Bisect, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        if let Some(e) = &self.bisect.error {
            ui.colored_label(t.danger, format!("Couldn't read the bisect again: {e}"));
            return;
        }
        match (&bisect.outcome, &bisect.progress, &bisect.marks.bad) {
            (Some(Outcome::FirstBad(hash)), _, _) => {
                ui.label(RichText::new("First bad commit:").color(t.text));
                self.commit_link(ui, hash, acts);
            }
            (Some(Outcome::OnlySkipped(possible)), _, _) => {
                ui.label(
                    RichText::new(format!(
                        "Only skipped commits are left: the first bad commit is one of these {}:",
                        possible.len()
                    ))
                    .color(t.text),
                );
                for hash in possible.iter().take(MAX_NAMED) {
                    self.commit_link(ui, hash, acts);
                }
                if possible.len() > MAX_NAMED {
                    ui.label(
                        RichText::new(format!("and {} more", possible.len() - MAX_NAMED))
                            .color(t.text_dim),
                    );
                }
            }
            (None, Some(progress), _) => {
                // The count first: the commit's summary is cut to fit.
                ui.label(RichText::new(steps_text(progress.steps, progress.left)).color(t.text));
                ui.label(RichText::new("· testing").color(t.text_dim));
                self.commit_link(ui, &bisect.current, acts);
            }
            (None, None, None) => {
                waiting(ui, &t, bisect.word(Mark::Bad), "has the change");
            }
            (None, None, Some(_)) => {
                waiting(ui, &t, bisect.word(Mark::Good), "is from before the change");
            }
        }
    }

    /// A commit's short hash and summary; a click selects it and shows its details.
    fn commit_link(&self, ui: &mut Ui, hash: &str, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let summary = self
            .index
            .get(hash)
            .and_then(|&i| self.commits.get(i))
            .map(|c| c.summary.as_str())
            .unwrap_or_default();
        let text = if summary.is_empty() {
            short(hash).to_owned()
        } else {
            format!("{} {summary}", short(hash))
        };
        let link = egui::Label::new(RichText::new(text).color(t.accent))
            .sense(egui::Sense::click())
            .truncate();
        let resp = ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.on_hover_text("Show this commit").clicked() {
            acts.push(Action::SelectHash(hash.to_owned()));
        }
    }

    /// Right to left: Stop (or Finish), Run a command…, Skip, Bad, Good.
    fn bisect_buttons(&self, ui: &mut Ui, bisect: &Bisect, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let running = self.bisect.running();
        let back = match bisect.start_branch() {
            Some(branch) => format!("git bisect reset: checks out {branch} again"),
            None => {
                "git bisect reset: checks out again what was checked out when it began".to_owned()
            }
        };
        if bisect.outcome.is_some() {
            if widgets::primary_button(ui, "Finish", idle, 80.0)
                .on_hover_text(&back)
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Reset));
            }
            return;
        }
        if ui
            .add_enabled(idle && !running, egui::Button::new("Stop"))
            .on_hover_text(&back)
            .clicked()
        {
            acts.push(self.stop_bisect_action());
        }
        let ready = bisect.progress.is_some();
        if ui
            .add_enabled(idle && ready, egui::Button::new("Run a command…"))
            .on_hover_text("git bisect run: a command tests each commit for you")
            .on_disabled_hover_text(if ready {
                "Wait for the operation going on to finish"
            } else {
                "Mark a good and a bad commit first"
            })
            .clicked()
        {
            acts.push(self.bisect_run_dialog());
        }
        let current = short(&bisect.current).to_owned();
        for (mark, tip) in [
            (
                Mark::Skip,
                format!("{current} can't be tested: try another one"),
            ),
            (Mark::Bad, format!("{current} has the change")),
            (Mark::Good, format!("{current} is from before the change")),
        ] {
            let word = capitalised(if mark == Mark::Skip {
                "skip"
            } else {
                bisect.word(mark)
            });
            if ui
                .add_enabled(idle && !running, egui::Button::new(word))
                .on_hover_text(tip)
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Mark { mark, rev: None }));
            }
        }
    }
}

/// What a bisect still needs before it can pick commits to test.
fn waiting(ui: &mut Ui, t: &Tokens, word: &str, meaning: &str) {
    let text = format!(
        "Mark a {word} commit, one that {meaning}: right-click it in the graph, or mark the one \
         checked out with the buttons."
    );
    ui.add(egui::Label::new(RichText::new(&text).color(t.text)).truncate())
        .on_hover_text(text);
}

/// "About 3 steps left (12 commits after this one)".
fn steps_text(steps: u32, left: usize) -> String {
    let steps = match steps {
        0 => "The last step".to_owned(),
        1 => "About 1 step left".to_owned(),
        n => format!("About {n} steps left"),
    };
    let left = match left {
        0 => "no commits after this one".to_owned(),
        1 => "1 commit after this one".to_owned(),
        n => format!("{n} commits after this one"),
    };
    format!("{steps} ({left})")
}

/// `good` → `Good`.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The run's command and how it is going, and its output (newest at the bottom).
fn run_log(ui: &mut Ui, run: &RunLog, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("git bisect run {}", run.command))
                .font(theme::mono(12.0))
                .color(t.text),
        );
        match &run.ended {
            None => {
                ui.add(crate::spinner::Spinner::new());
                ui.label(RichText::new("running…").color(t.text_dim));
            }
            Some(Ok(e)) if e.stopped => {
                ui.label(RichText::new("stopped").color(t.text_dim));
            }
            Some(Ok(e)) => {
                let (text, color) = match e.code {
                    Some(0) => ("done".to_owned(), t.success),
                    Some(c) => (format!("ended with exit code {c}"), t.warning),
                    None => ("ended by a signal".to_owned(), t.warning),
                };
                ui.label(RichText::new(text).color(color));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("failed: {e}")).color(t.danger));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if run.ended.is_some() {
                if ui.button("Close").clicked() {
                    acts.push(Action::Bisect(BisectAct::CloseLog));
                }
            } else if ui
                .button("Stop run")
                .on_hover_text(
                    "Stops the command and what it started; the bisect stays where it got to",
                )
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::StopRun));
            }
            let toggle = if run.open {
                "Hide output"
            } else {
                "Show output"
            };
            if ui.button(toggle).clicked() {
                acts.push(Action::Bisect(BisectAct::ToggleLog));
            }
        });
    });
    if run.open {
        output(ui, run);
    }
}

fn output(ui: &mut Ui, run: &RunLog) {
    let t = Tokens::get(ui.ctx());
    let Ok(lines) = run.lines.lock() else {
        return;
    };
    let font = theme::mono(11.5);
    let row_h = ui.fonts_mut(|f| f.row_height(&font));
    egui::Frame::new()
        .fill(t.card)
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt("bisect_run_output")
                .max_height(LOG_H)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show_rows(ui, row_h, lines.len(), |ui, range| {
                    for line in lines.range(range) {
                        ui.label(RichText::new(line).font(font.clone()).color(t.text_dim));
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_read_as_words() {
        assert_eq!(
            steps_text(3, 12),
            "About 3 steps left (12 commits after this one)"
        );
        assert_eq!(
            steps_text(1, 1),
            "About 1 step left (1 commit after this one)"
        );
        assert_eq!(
            steps_text(0, 0),
            "The last step (no commits after this one)"
        );
    }

    #[test]
    fn terms_become_button_labels() {
        assert_eq!(capitalised("good"), "Good");
        assert_eq!(capitalised("broken"), "Broken");
        assert_eq!(capitalised(""), "");
    }
}
