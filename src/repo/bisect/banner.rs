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
const ONE_ROW_W: f32 = 760.0;

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
        widgets::chip(
            ui,
            crate::i18n::text("repo-workflows-bisect-bisect-status-675fd45e"),
            t.accent,
        );
        ui.add_space(4.0);
        self.where_it_is(ui, bisect, acts);
    }

    /// The commit being tested and the steps left, what is missing to start, or the outcome.
    fn where_it_is(&self, ui: &mut Ui, bisect: &Bisect, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        if let Some(e) = &self.bisect.error {
            ui.add(egui::Label::new(RichText::new(e).color(t.danger)).truncate());
            return;
        }
        match (&bisect.outcome, &bisect.progress, &bisect.marks.bad) {
            (Some(Outcome::FirstBad(hash)), _, _) => {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bisect-where-it-is-8b3812c0",
                    ))
                    .color(t.text),
                );
                self.commit_link(ui, hash, acts);
            }
            (Some(Outcome::OnlySkipped(possible)), _, _) => {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-bisect-where-it-is-25230d8d",
                        &[("possible-len-1", (possible.len()).clone().into())],
                    ))
                    .color(t.text),
                );
                // Short hashes only, so that they all fit.
                for hash in possible.iter().take(MAX_NAMED) {
                    hash_link(ui, hash, acts);
                }
                if possible.len() > MAX_NAMED {
                    ui.label(
                        RichText::new(crate::i18n::message(
                            "repo-workflows-bisect-where-it-is-683768ab",
                            &[(
                                "possible-len-max-named-1",
                                (possible.len() - MAX_NAMED).clone().into(),
                            )],
                        ))
                        .color(t.text_dim),
                    );
                }
            }
            (None, Some(progress), _) => {
                // The count first: the commit's summary is cut to fit.
                ui.label(RichText::new(steps_text(progress.steps, progress.left)).color(t.text));
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-bisect-where-it-is-7d7c30e4",
                    ))
                    .color(t.text_dim),
                );
                self.commit_link(ui, &bisect.current, acts);
            }
            (None, None, None) => {
                waiting(
                    ui,
                    &t,
                    bisect.word(Mark::Bad),
                    crate::i18n::text("repo-workflows-bisect-where-it-is-d5ebaa0a"),
                );
            }
            (None, None, Some(_)) if !bisect.marks.good.is_empty() => {
                let text = crate::i18n::text("repo-workflows-bisect-where-it-is-4c0b8e59");
                ui.add(egui::Label::new(RichText::new(text).color(t.warning)).truncate())
                    .on_hover_text(text);
            }
            (None, None, Some(_)) => {
                waiting(
                    ui,
                    &t,
                    bisect.word(Mark::Good),
                    crate::i18n::text("repo-workflows-bisect-where-it-is-94883ca6"),
                );
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
        if resp
            .on_hover_text(crate::i18n::text(
                "repo-workflows-bisect-commit-link-63633913",
            ))
            .clicked()
        {
            acts.push(Action::SelectHash(hash.to_owned()));
        }
    }

    /// Right to left: Stop (or Finish), Run a command…, Skip, Bad, Good.
    fn bisect_buttons(&self, ui: &mut Ui, bisect: &Bisect, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let running = self.bisect.running();
        let back = match bisect.start_branch() {
            Some(branch) => crate::i18n::message(
                "repo-workflows-bisect-bisect-buttons-0599842f",
                &[("branch", (branch).clone().into())],
            ),
            None => crate::i18n::text("repo-workflows-bisect-bisect-buttons-a28c79f7").to_owned(),
        };
        if bisect.outcome.is_some() {
            if widgets::primary_button(
                ui,
                crate::i18n::text("repo-workflows-bisect-bisect-buttons-a6c7a84b"),
                idle,
                80.0,
            )
            .on_hover_text(&back)
            .on_disabled_hover_text(crate::i18n::text(BUSY))
            .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Reset));
            }
            return;
        }
        let wait = crate::i18n::text(if running { RUNNING } else { BUSY });
        if ui
            .add_enabled(
                idle && !running,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-bisect-bisect-buttons-cae7d57b",
                )),
            )
            .on_hover_text(&back)
            .on_disabled_hover_text(wait)
            .clicked()
        {
            acts.push(self.stop_bisect_action());
        }
        let ready = bisect.progress.is_some();
        self.parallel_banner_button(ui, acts);
        if ui
            .add_enabled(
                idle && ready,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-bisect-bisect-buttons-eded3d42",
                )),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-bisect-bisect-buttons-7cf0c74e",
            ))
            .on_disabled_hover_text(if ready {
                wait
            } else {
                crate::i18n::text("repo-workflows-bisect-bisect-buttons-c3efc1be")
            })
            .clicked()
        {
            acts.push(self.bisect_run_dialog());
        }
        let current = short(&bisect.current).to_owned();
        for (mark, tip) in [
            (
                Mark::Skip,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-buttons-12c0910a",
                    &[("current", (current).clone().into())],
                ),
            ),
            (
                Mark::Bad,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-buttons-7f573858",
                    &[("current", (current).clone().into())],
                ),
            ),
            (
                Mark::Good,
                crate::i18n::message(
                    "repo-workflows-bisect-bisect-buttons-a9a77839",
                    &[("current", (current).clone().into())],
                ),
            ),
        ] {
            let word = capitalised(if mark == Mark::Skip {
                "skip"
            } else {
                bisect.word(mark)
            });
            if ui
                .add_enabled(idle && !running, egui::Button::new(word))
                .on_hover_text(tip)
                .on_disabled_hover_text(wait)
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::Mark { mark, rev: None }));
            }
        }
    }
}

/// Why the banner's buttons are greyed out while another operation runs.
pub(super) const BUSY: &str = "repo-workflows-bisect--e5e3ecc5";

/// Why they are while `git bisect run` tests the commits.
pub(super) const RUNNING: &str = "repo-workflows-bisect--512af075";

/// A commit's short hash; a click selects it and shows its details.
fn hash_link(ui: &mut Ui, hash: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let link =
        egui::Label::new(RichText::new(short(hash)).color(t.accent)).sense(egui::Sense::click());
    let resp = ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp
        .on_hover_text(crate::i18n::text(
            "repo-workflows-bisect-hash-link-63633913",
        ))
        .clicked()
    {
        acts.push(Action::SelectHash(hash.to_owned()));
    }
}

/// What a bisect still needs before it can pick commits to test.
fn waiting(ui: &mut Ui, t: &Tokens, word: &str, meaning: &str) {
    let text = crate::i18n::message(
        "repo-workflows-bisect-waiting-2d4ce59a",
        &[
            ("word", (word).clone().into()),
            ("meaning", (meaning).clone().into()),
            ("capitalised-word-1", (capitalised(word)).clone().into()),
        ],
    );
    ui.add(egui::Label::new(RichText::new(&text).color(t.text)).truncate())
        .on_hover_text(text);
}

/// "About 3 steps left (12 commits after this one)".
fn steps_text(steps: u32, left: usize) -> String {
    let steps = match steps {
        0 => crate::i18n::text("repo-workflows-bisect-steps-text-2d1513e9").to_owned(),
        1 => crate::i18n::text("repo-workflows-bisect-steps-text-263b5d8b").to_owned(),
        n => crate::i18n::message(
            "repo-workflows-bisect-steps-text-14999716",
            &[("n", (n).clone().into())],
        ),
    };
    let left = match left {
        0 => crate::i18n::text("repo-workflows-bisect-steps-text-a4e9b171").to_owned(),
        1 => crate::i18n::text("repo-workflows-bisect-steps-text-9bbc8458").to_owned(),
        n => crate::i18n::message(
            "repo-workflows-bisect-steps-text-5ccbfd65",
            &[("n", (n).clone().into())],
        ),
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
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-bisect-run-log-14aae160"))
                        .color(t.text_dim),
                );
            }
            Some(Ok(e)) if e.stopped => {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-bisect-run-log-8322e87d"))
                        .color(t.text_dim),
                );
            }
            Some(Ok(e)) => {
                let (text, color) = match e.code {
                    Some(0) => ("done".to_owned(), t.success),
                    Some(c) => (
                        crate::i18n::message(
                            "repo-workflows-bisect-run-log-534a3a34",
                            &[("c", (c).clone().into())],
                        ),
                        t.warning,
                    ),
                    None => (
                        crate::i18n::text("repo-workflows-bisect-run-log-bb5e4bf5").to_owned(),
                        t.warning,
                    ),
                };
                ui.label(RichText::new(text).color(color));
            }
            Some(Err(e)) => {
                ui.label(
                    RichText::new(crate::i18n::message(
                        "repo-workflows-bisect-run-log-5fd804ed",
                        &[("e", (e).clone().into())],
                    ))
                    .color(t.danger),
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if run.ended.is_some() {
                if ui
                    .button(crate::i18n::text("repo-workflows-bisect-run-log-7d9eb7ac"))
                    .clicked()
                {
                    acts.push(Action::Bisect(BisectAct::CloseLog));
                }
            } else if ui
                .button(crate::i18n::text("repo-workflows-bisect-run-log-b7ec68a2"))
                .on_hover_text(crate::i18n::text("repo-workflows-bisect-run-log-8d23f4ee"))
                .clicked()
            {
                acts.push(Action::Bisect(BisectAct::StopRun));
            }
            let toggle = if run.open {
                crate::i18n::text("repo-workflows-bisect-run-log-64876c47")
            } else {
                crate::i18n::text("repo-workflows-bisect-run-log-9dbbb249")
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
