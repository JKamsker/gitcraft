//! The banner over the graph while a parallel bisect runs (the machine, the round, how many
//! commits are left, Cancel), and once it ended (what it found, Close).

use egui::{RichText, Ui};

use super::{ParallelAct, View};
use crate::parallel_bisect::Outcome;
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::Tokens;

/// The most commits named when skipped ones leave a choice.
const MAX_NAMED: usize = 8;

/// How much of a test's output a hover shows: its last lines.
const OUTPUT_LINES: usize = 20;

impl RepoTab {
    pub(in crate::repo) fn parallel_banner(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some(view) = self.parallel.view() else {
            return;
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self.parallel_buttons(ui, view, acts);
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                let t = Tokens::get(ui.ctx());
                crate::icons::inline(ui, "flask-conical", 14.0, t.accent);
                widgets::chip(ui, "Parallel bisect", t.accent);
                ui.add_space(4.0);
                match &view.outcome {
                    None => progress(ui, view, &t),
                    Some(outcome) => self.parallel_outcome(ui, outcome, &view.machine, acts),
                }
            });
        });
        if let Some(warning) = &view.warning {
            let t = Tokens::get(ui.ctx());
            ui.label(RichText::new(warning).small().color(t.warning));
        }
    }

    /// Right to left: Cancel while it runs; Close (and Mark in the bisect) once it ended.
    fn parallel_buttons(&self, ui: &mut Ui, view: &View, acts: &mut Vec<Action>) {
        if view.running() {
            if ui
                .add_enabled(
                    !view.cancelling,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-parallel_bisect-parallel-buttons-19766ed6",
                    )),
                )
                .on_disabled_hover_text("Stopping the tests and removing the worktrees…")
                .on_hover_text("Stops every test running and removes the worktrees")
                .clicked()
            {
                acts.push(Action::ParallelBisect(ParallelAct::Cancel));
            }
            return;
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-parallel_bisect-parallel-buttons-7d9eb7ac",
            ))
            .clicked()
        {
            acts.push(Action::ParallelBisect(ParallelAct::Close));
        }
        // Only between the same marks: another range may say nothing of the bisect's.
        let bisecting = self
            .bisect
            .state()
            .is_some_and(|b| b.outcome.is_none() && view.same_ends(b));
        if bisecting && matches!(view.outcome, Some(Outcome::FirstBad(_))) {
            let idle = self.busy_op.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Mark in the bisect"))
                .on_hover_text(
                    "Marks the first bad commit bad and its parents good in the bisect going on, \
                     which then ends there too",
                )
                .on_disabled_hover_text(crate::i18n::text(
                    "repo-workflows-parallel_bisect-parallel-buttons-e5e3ecc5",
                ))
                .clicked()
            {
                acts.push(Action::ParallelBisect(ParallelAct::MarkInBisect));
            }
        }
    }

    /// What it found, or why it found nothing.
    fn parallel_outcome(
        &self,
        ui: &mut Ui,
        outcome: &Outcome,
        machine: &str,
        acts: &mut Vec<Action>,
    ) {
        let t = Tokens::get(ui.ctx());
        match outcome {
            Outcome::FirstBad(hash) => {
                ui.label(
                    RichText::new(crate::i18n::text(
                        "repo-workflows-parallel_bisect-parallel-outcome-8b3812c0",
                    ))
                    .color(t.text),
                );
                self.parallel_link(ui, hash, acts);
            }
            Outcome::OneOf(hashes) => {
                ui.label(
                    RichText::new(format!(
                        "Only skipped commits are left: the first bad commit is one of these {}:",
                        hashes.len()
                    ))
                    .color(t.text),
                );
                for hash in hashes.iter().take(MAX_NAMED) {
                    hash_link(ui, hash, acts);
                }
                if hashes.len() > MAX_NAMED {
                    ui.label(
                        RichText::new(crate::i18n::message(
                            "repo-workflows-parallel_bisect-parallel-outcome-683768ab",
                            &[(
                                "hashes-len-max-named-1",
                                (hashes.len() - MAX_NAMED).clone().into(),
                            )],
                        ))
                        .color(t.text_dim),
                    );
                }
            }
            Outcome::GoodEndFails { hash, output } => {
                let text = format!(
                    "The good end already fails: {} fails the test on {machine}, so there is \
                     nothing to look for after it. Check the command, or mark an older good \
                     commit.",
                    short(hash)
                );
                warning(ui, &text, output, &t);
            }
            Outcome::BadEndPasses { hash, output } => {
                let text = format!(
                    "The bad end passes: {} passes the test on {machine}, so the test doesn't \
                     show the problem there. Check the command.",
                    short(hash)
                );
                warning(ui, &text, output, &t);
            }
            Outcome::Cancelled => {
                ui.label(
                    RichText::new("Cancelled: the tests were stopped and the worktrees removed.")
                        .color(t.text_dim),
                );
            }
            Outcome::Failed(e) => {
                let first = e.lines().next().unwrap_or_default();
                ui.add(egui::Label::new(RichText::new(first).color(t.danger)).truncate())
                    .on_hover_text(e.as_str());
            }
        }
    }

    /// A commit's short hash and summary; a click selects it.
    fn parallel_link(&self, ui: &mut Ui, hash: &str, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let summary = self
            .index
            .get(hash)
            .and_then(|&i| self.commits.get(i))
            .map(|c| c.summary.as_str())
            .unwrap_or_default();
        let text = format!("{} {summary}", short(hash));
        let link = egui::Label::new(RichText::new(text.trim_end()).color(t.accent))
            .sense(egui::Sense::click())
            .truncate();
        let resp = ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp
            .on_hover_text(crate::i18n::text(
                "repo-workflows-parallel_bisect-parallel-link-63633913",
            ))
            .clicked()
        {
            acts.push(Action::SelectHash(hash.to_owned()));
        }
    }
}

/// While it runs: what it is doing, or its round and how many commits are left.
fn progress(ui: &mut Ui, view: &View, t: &Tokens) {
    ui.add(crate::spinner::Spinner::new());
    let text = match &view.step {
        Some(step) => step.clone(),
        None => progress_text(view),
    };
    ui.add(egui::Label::new(RichText::new(&text).color(t.text)).truncate())
        .on_hover_text(text.as_str());
}

/// "Round 2, about 1 more · 37 of 412 commits left · 8 at once on SSH · build".
fn progress_text(view: &View) -> String {
    let round = match (view.round, view.rounds_left) {
        (0, _) => "Getting ready".to_owned(),
        (n, 0) => format!("Round {n}, the last"),
        (n, 1) => format!("Round {n}, about 1 more"),
        (n, more) => format!("Round {n}, about {more} more"),
    };
    let left = match view.remaining.len() {
        1 => "1 commit left".to_owned(),
        n => format!("{n} of {} commits left", view.total),
    };
    let testing = match view.testing.len() {
        0 => String::new(),
        1 => " · testing 1".to_owned(),
        n => format!(" · testing {n}"),
    };
    format!(
        "{round} · {left}{testing} · {} at once on {}",
        view.parallel, view.machine
    )
}

/// An end the test says the opposite of, the end of its output on hover.
fn warning(ui: &mut Ui, text: &str, output: &str, t: &Tokens) {
    let resp = ui.add(egui::Label::new(RichText::new(text).color(t.warning)).truncate());
    let shown = last_lines(output, OUTPUT_LINES);
    if shown.is_empty() {
        resp.on_hover_text(text);
    } else {
        resp.on_hover_text(format!("{text}\n\nThe test said:\n{shown}"));
    }
}

/// A commit's short hash; a click selects it.
fn hash_link(ui: &mut Ui, hash: &str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let link =
        egui::Label::new(RichText::new(short(hash)).color(t.accent)).sense(egui::Sense::click());
    let resp = ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand);
    if resp
        .on_hover_text(crate::i18n::text(
            "repo-workflows-parallel_bisect-hash-link-63633913",
        ))
        .clicked()
    {
        acts.push(Action::SelectHash(hash.to_owned()));
    }
}

/// The last `n` lines of `text`.
pub(super) fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parallel_bisect::{Machine, Setup};
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::time::Duration;

    fn view() -> View {
        View::new(&Setup {
            repo: "/r".into(),
            bad: "b".into(),
            good: vec!["g".into()],
            skip: vec![],
            command: "make".into(),
            machine: Machine::Here,
            repo_there: None,
            parallel: 8,
            timeout: Duration::from_secs(60),
        })
    }

    #[test]
    fn progress_says_the_round_and_what_is_left() {
        let mut v = view();
        assert_eq!(
            progress_text(&v),
            "Getting ready · 0 of 0 commits left · 8 at once on this computer"
        );
        v.round = 2;
        v.rounds_left = 1;
        v.total = 412;
        v.remaining = Arc::new((0..37).map(|i| i.to_string()).collect::<HashSet<_>>());
        v.testing = ["a", "b"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(
            progress_text(&v),
            "Round 2, about 1 more · 37 of 412 commits left · testing 2 · 8 at once on this \
             computer"
        );
        v.rounds_left = 0;
        v.remaining = Arc::new(std::iter::once("x".to_owned()).collect());
        v.testing.clear();
        assert_eq!(
            progress_text(&v),
            "Round 2, the last · 1 commit left · 8 at once on this computer"
        );
    }

    #[test]
    fn only_the_last_lines_of_an_output_show() {
        assert_eq!(last_lines("a\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("a", 5), "a");
        assert_eq!(last_lines("", 5), "");
    }
}
