//! Bisecting from the tab: the bisect going on (read again after every reload, so that one
//! started in a terminal shows too), its marks and steps run in the background, and
//! `git bisect run` with its output as it comes. The banner and the graph's marks are drawn
//! from what is read here.

mod run_log;
use run_log::*;
mod banner;
mod items;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::messages::{Msg, spawn_or};
use super::{Action, Dialog, RepoTab, ops, short};
use crate::git::{self, Bisect, Mark, Outcome};

pub(super) use items::mark_color;

/// How many lines of a run's output are kept (the oldest go first).
const KEPT_LINES: usize = 5000;

/// A run that takes longer than this gets a notification when it ends.
const LONG_RUN: Duration = Duration::from_secs(4);

/// What the views ask of the bisect.
pub(crate) enum BisectAct {
    /// Start one between the commit `bad` and, when given, the commit `good`.
    Start { bad: String, good: Option<String> },
    /// Mark `rev` (the commit being tested when `None`).
    Mark { mark: Mark, rev: Option<String> },
    /// End it and check out again what was checked out when it began.
    Reset,
    /// `git bisect run` with this command.
    Run(String),
    /// Stop the run going on.
    StopRun,
    /// Show or hide the run's output.
    ToggleLog,
    /// Let go of the output of a run that ended.
    CloseLog,
}

/// The tab's bisect.
#[derive(Default)]
pub(super) struct Bisecting {
    /// The bisect going on, as last read (`None`: none).
    state: Option<Bisect>,
    /// Why it couldn't be read the last time.
    error: Option<String>,
    /// Numbers the reads, so that an older answer doesn't replace a newer one.
    reads: u64,
    shown: u64,
    /// The `git bisect run` going on, or the last one, with its output.
    run: Option<RunLog>,
    /// The command last run, offered again.
    last_command: String,
}

impl Bisecting {
    /// The bisect going on, if any.
    pub(super) fn state(&self) -> Option<&Bisect> {
        self.state.as_ref()
    }

    /// What `hash` was marked in the bisect going on.
    pub(super) fn mark_of(&self, hash: &str) -> Option<Mark> {
        self.state.as_ref()?.mark_of(hash)
    }

    /// Whether `hash` is the first bad commit it found (or one it could be).
    pub(super) fn is_culprit(&self, hash: &str) -> bool {
        match self.state.as_ref().and_then(|b| b.outcome.as_ref()) {
            Some(Outcome::FirstBad(h)) => h == hash,
            Some(Outcome::OnlySkipped(possible)) => possible.iter().any(|h| h == hash),
            None => false,
        }
    }

    /// Whether the banner shows: a bisect is going on (a later read that failed says so
    /// there), or a run's output is still there.
    pub(super) fn shows(&self) -> bool {
        self.state.is_some() || self.run.is_some()
    }

    /// Whether `git bisect run` is going on.
    pub(super) fn running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.ended.is_none())
    }
}

impl RepoTab {
    pub(super) fn bisect_act(&mut self, act: BisectAct, ctx: &egui::Context) {
        match act {
            BisectAct::Start { bad, good } => self.start_bisect(bad, good, ctx),
            BisectAct::Mark { mark, rev } => self.mark_bisect(mark, rev, ctx),
            BisectAct::Reset => self.reset_bisect(ctx),
            BisectAct::Run(command) => self.start_bisect_run(command, ctx),
            BisectAct::StopRun => {
                if let Some(run) = &self.bisect.run {
                    run.stopper.stop();
                }
            }
            BisectAct::ToggleLog => {
                if let Some(run) = &mut self.bisect.run {
                    run.open = !run.open;
                }
            }
            BisectAct::CloseLog => {
                if !self.bisect.running() {
                    self.bisect.run = None;
                }
            }
        }
    }

    /// Ends the bisect: Finish once it found something, else Stop.
    fn reset_bisect(&mut self, ctx: &egui::Context) {
        let state = self.bisect.state.as_ref();
        let label = if state.is_some_and(|b| b.outcome.is_some()) {
            "Finish bisect"
        } else {
            "Stop bisect"
        };
        let back = match state.and_then(Bisect::start_branch) {
            Some(branch) => crate::i18n::message(
                "repo-workflows-bisect-reset-bisect-fd58b76b",
                &[("branch", (branch).into())],
            ),
            None => crate::i18n::text("repo-workflows-bisect-reset-bisect-b63b6486").to_owned(),
        };
        self.bisect_step(label, ctx, move |path| {
            git::bisect_reset(path).map(|_| back)
        });
    }

    /// Why a bisect can't start now, if it can't.
    pub(super) fn bisect_blocker(&self) -> Option<String> {
        if let Some(busy) = &self.busy_op {
            return Some(crate::i18n::message(
                "repo-workflows-bisect-bisect-blocker-cff28fea",
                &[("busy", (busy).clone().into())],
            ));
        }
        git::start_blocker(&self.work, self.bisect.state.is_some())
    }

    fn start_bisect(&mut self, bad: String, good: Option<String>, ctx: &egui::Context) {
        if let Some(why) = self.bisect_blocker() {
            self.status = Some(crate::repo::Notice::plain((
                format!("Can't start a bisect: {why}"),
                true,
            )));
            return;
        }
        self.bisect_step("Start bisect", ctx, move |path| {
            git::bisect_start(path, &bad, good.as_deref())
        });
    }

    fn mark_bisect(&mut self, mark: Mark, rev: Option<String>, ctx: &egui::Context) {
        let Some(bisect) = self.bisect.state.clone() else {
            self.status = Some(crate::repo::Notice::plain((
                "No bisect is going on".to_owned(),
                true,
            )));
            return;
        };
        let label = match (&rev, mark) {
            (Some(rev), _) => format!("Mark {} as {}", short(rev), bisect.word(mark)),
            (None, Mark::Skip) => "Skip this commit".to_owned(),
            (None, _) => format!("Mark as {}", bisect.word(mark)),
        };
        self.bisect_step(&label, ctx, move |path| {
            git::bisect_mark(path, &bisect, mark, rev.as_deref())
        });
    }

    /// Runs one bisect command (`work`) in the background as the tab's operation, then says
    /// how it went and reads the repository again.
    fn bisect_step(
        &mut self,
        label: &str,
        ctx: &egui::Context,
        work: impl FnOnce(&std::path::Path) -> Result<String, String> + Send + 'static,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        self.status = None;
        let path = self.path.clone();
        let label = label.to_owned();
        let failed = {
            let label = label.clone();
            move |e| step_done(label, Err(e))
        };
        spawn_or(&self.tx, ctx, failed, move || step_done(label, work(&path)));
    }

    fn bisect_step_done(
        &mut self,
        label: &str,
        result: Result<String, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        self.status = Some(crate::repo::Notice::plain(match result {
            Ok(said) => (format!("{label}: {}", said_briefly(&said)), false),
            Err(e) => (format!("{label} failed: {e}"), true),
        }));
        self.reload(ctx);
    }

    /// Reads the bisect again, in the background (after every reload).
    pub(super) fn read_bisect(&mut self, ctx: &egui::Context) {
        self.bisect.reads += 1;
        let read = self.bisect.reads;
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::bisect_state(&path),
            move |tab, result, ctx| tab.bisect_read(read, result, ctx),
        );
    }

    fn bisect_read(
        &mut self,
        read: u64,
        result: Result<Option<Bisect>, String>,
        ctx: &egui::Context,
    ) {
        if read <= self.bisect.shown {
            return;
        }
        // The first read finds what was there already: an end found before isn't news.
        let first = self.bisect.shown == 0;
        self.bisect.shown = read;
        let state = match result {
            Ok(state) => state,
            Err(e) => {
                self.bisect.error = Some(e);
                return;
            }
        };
        self.bisect.error = None;
        let was_over = self
            .bisect
            .state
            .as_ref()
            .is_some_and(|b| b.outcome.is_some());
        // Just found: the first bad commit's details show.
        if !first
            && !was_over
            && let Some(Outcome::FirstBad(hash)) = state.as_ref().and_then(|b| b.outcome.as_ref())
        {
            self.goto(&hash.clone(), ctx);
        }
        self.bisect.state = state;
    }

    fn start_bisect_run(&mut self, command: String, ctx: &egui::Context) {
        let command = command.trim().to_owned();
        if command.is_empty() || self.busy_op.is_some() || self.bisect.running() {
            return;
        }
        self.busy_op = Some("Bisect run".to_owned());
        self.op_started = Some(Instant::now());
        self.status = None;
        self.bisect.last_command = command.clone();
        let log = RunLog::new(command.clone());
        let (lines, stopper) = (Arc::clone(&log.lines), log.stopper.clone());
        self.bisect.run = Some(log);
        let path = self.path.clone();
        let repaint = ctx.clone();
        let failed = |e| run_done(Err(e));
        spawn_or(&self.tx, ctx, failed, move || {
            let args = git::bisect_run_args(&command);
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let ended = git::stream(&path, &args, &stopper, |line| {
                keep_line(&lines, line);
                repaint.request_repaint();
            });
            run_done(ended)
        });
    }

    fn bisect_run_done(&mut self, ended: Result<git::Ended, String>, ctx: &egui::Context) {
        self.busy_op = None;
        self.op_started = None;
        let Some(run) = &mut self.bisect.run else {
            return;
        };
        let (last_line, only_skipped) = run
            .lines
            .lock()
            .map(|lines| {
                // git's own error says why, more than the line printed last.
                let last = (lines.iter().rev())
                    .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
                    .or_else(|| lines.iter().rev().find(|l| !l.trim().is_empty()))
                    .cloned();
                (
                    last.unwrap_or_default(),
                    lines.iter().any(|l| l.starts_with("There are only")),
                )
            })
            .unwrap_or_default();
        let (message, failed) = run_message(&ended, &last_line, only_skipped);
        if run.started.elapsed() > LONG_RUN {
            self.finished.push(crate::repo::Notice::plain((
                format!("{} · {message}", self.name),
                failed,
            )));
        }
        run.ended = Some(ended);
        self.status = Some(crate::repo::Notice::plain((message, failed)));
        self.reload(ctx);
    }

    /// The command offered in the run dialog: the last one run.
    pub(super) fn bisect_run_dialog(&self) -> Action {
        Action::Dialog(Dialog::bisect_run(&self.bisect.last_command))
    }

    /// Asks before stopping a bisect not finished yet: the marks made go with it.
    pub(super) fn stop_bisect_action(&self) -> Action {
        let over = self
            .bisect
            .state
            .as_ref()
            .is_some_and(|b| b.outcome.is_some());
        if over {
            return Action::Bisect(BisectAct::Reset);
        }
        let back = self
            .bisect
            .state
            .as_ref()
            .and_then(|b| b.start_branch().map(str::to_owned))
            .unwrap_or_else(|| {
                crate::i18n::text("repo-workflows-bisect-stop-bisect-action-fff2c8f7").to_owned()
            });
        Action::Dialog(Dialog::confirm(
            "Stop bisect",
            crate::i18n::message(
                "repo-workflows-bisect-stop-bisect-action-c380662a",
                &[("back", (back).clone().into())],
            ),
            crate::i18n::text("repo-workflows-bisect-stop-bisect-action-0ddac35c"),
            ops(&[&["bisect", "reset"]]),
            None,
        ))
    }
}

fn step_done(label: String, result: Result<String, String>) -> Msg {
    Msg::Then(Box::new(move |tab, ctx| {
        tab.bisect_step_done(&label, result, ctx)
    }))
}

fn run_done(ended: Result<git::Ended, String>) -> Msg {
    Msg::Then(Box::new(move |tab, ctx| tab.bisect_run_done(ended, ctx)))
}

/// What git said after a mark, in a line: where the bisect is now (`Bisecting: 3 revisions
/// left…`), or how it ended.
fn said_briefly(said: &str) -> String {
    let lines: Vec<&str> = said.lines().map(str::trim).collect();
    let pick = lines
        .iter()
        .find(|l| l.contains(" is the first ") && l.ends_with(" commit"))
        .or_else(|| lines.iter().find(|l| l.starts_with("There are only")))
        .or_else(|| lines.iter().find(|l| l.starts_with("Bisecting:")))
        .or_else(|| lines.iter().find(|l| !l.is_empty()));
    match pick {
        // In the banner's words: "4 commits left to test after this (about 2 steps)".
        Some(line) => match line.strip_prefix("Bisecting: ") {
            Some(rest) => rest
                .replace("revisions", "commits")
                .replace("revision", "commit")
                .replace("roughly", "about"),
            None => line.to_string(),
        },
        None => "done".to_owned(),
    }
}

/// How a run ended, for the status line: the message, and whether it failed.
/// `only_skipped`: git said only skipped commits are left, which ends a run too.
fn run_message(
    ended: &Result<git::Ended, String>,
    last_line: &str,
    only_skipped: bool,
) -> (String, bool) {
    match ended {
        Ok(e) if e.stopped => (
            crate::i18n::text("repo-workflows-bisect-run-message-96334386").to_owned(),
            false,
        ),
        Ok(e) if e.code == Some(0) => (
            crate::i18n::text("repo-workflows-bisect-run-message-de2346a5").to_owned(),
            false,
        ),
        Ok(_) if only_skipped => (
            crate::i18n::text("repo-workflows-bisect-run-message-dc9c7971").to_owned(),
            false,
        ),
        Ok(e) => {
            let code = e.code.map_or_else(
                || crate::i18n::text("repo-workflows-bisect-run-message-e8e581bc").to_owned(),
                |c| {
                    crate::i18n::message(
                        "repo-workflows-bisect-run-message-b1d63bd2",
                        &[("c", (c).into())],
                    )
                },
            );
            (
                crate::i18n::message(
                    "repo-workflows-bisect-run-message-b4eba258",
                    &[
                        ("code", (code).clone().into()),
                        ("last-line", (last_line).into()),
                    ],
                ),
                true,
            )
        }
        Err(e) => (
            crate::i18n::message(
                "repo-workflows-bisect-run-message-3a24171a",
                &[("e", (e).clone().into())],
            ),
            true,
        ),
    }
}

#[cfg(test)]
mod said_tests {
    use super::said_briefly;

    #[test]
    fn git_s_answer_to_a_mark_reads_in_the_banner_s_words() {
        assert_eq!(
            said_briefly(
                "Bisecting: 4 revisions left to test after this (roughly 2 steps)\n[abc] c5"
            ),
            "4 commits left to test after this (about 2 steps)"
        );
        assert_eq!(
            said_briefly("Bisecting: 1 revision left to test after this (roughly 1 step)"),
            "1 commit left to test after this (about 1 step)"
        );
        assert_eq!(
            said_briefly("abc is the first bad commit\ncommit abc\n"),
            "abc is the first bad commit"
        );
        assert_eq!(
            said_briefly("abc is the first 'bad' commit\ncommit abc\n"),
            "abc is the first 'bad' commit"
        );
        assert_eq!(said_briefly(""), "done");
    }
}
