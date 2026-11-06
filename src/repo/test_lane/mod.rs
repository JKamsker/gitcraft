//! The test lane: a pass / fail dot by each commit for the repository's test command
//! (`test.default.command`, as git-test keeps it), and *Test commits…*, which fills it in the
//! background, in scratch worktrees on the repository's machine, while the tab stays usable.
//! Results are git-test's notes, kept by tree, read again after every reload.

mod banner;
mod dot;
mod output;
#[cfg(test)]
mod tests;

pub(crate) use dot::LANE_W;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::messages::Msg;
use super::{Action, Dialog, RepoTab, short};
use crate::git::{self, RunStop, RunSummary, TestEnd, TestEvent, TestPlan, Verdict};
use crate::settings::Settings;

/// What the header's menu and the settings say of the lane.
pub(crate) const TEST_LANE_TIP: &str =
    "A pass / fail dot by each commit for the test command; Test commits… fills it in";

/// A run that takes longer than this gets a notification when it ends.
const LONG_RUN: Duration = Duration::from_secs(4);

/// How many commits are tested at once unless asked otherwise.
pub(crate) const DEFAULT_WORKERS: usize = 2;

/// What the views ask of the lane.
#[derive(Clone, Debug)]
pub(crate) enum LaneAct {
    /// Test the commits of `range` with `command`.
    Start(StartTests),
    /// Stop the run going on.
    Stop,
    /// Let go of the banner of a run that ended.
    Close,
    /// Show what the test of the tree printed.
    ShowOutput(String),
    CloseOutput,
}

/// A run asked for in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StartTests {
    /// `base..tip`, or a commit.
    pub range: String,
    pub command: String,
    pub workers: usize,
    /// Run trees again that have a result.
    pub retest: bool,
    /// Forget the results stored for the command before.
    pub forget: bool,
}

/// What is known of a tree for the lane's dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DotState {
    Good,
    Bad,
    /// Its test is going on.
    Running,
    /// It is waiting its turn.
    Queued,
    /// It couldn't be tested.
    Failed,
}

/// What a test printed, and how it ended, for the output window.
#[derive(Clone, Debug)]
pub(super) struct Kept {
    commit: String,
    end: TestEnd,
    lines: Vec<String>,
}

/// The tab's lane.
#[derive(Default)]
pub(crate) struct TestLane {
    /// It shows (from the settings, as of the last frame).
    pub(super) visible: bool,
    /// The repository's test command, as last read.
    command: Option<String>,
    /// Verdicts by tree, as last read from the notes, with the run's on top.
    results: HashMap<String, Verdict>,
    /// Why they couldn't be read the last time.
    error: Option<String>,
    /// Numbers the reads, so that an older answer doesn't replace a newer one.
    reads: u64,
    shown: u64,
    /// The run going on, or the last one, until its banner is closed.
    run: Option<LaneRun>,
    /// Numbers the runs, so that a stopped one's late events are not taken for the next's.
    runs: u64,
    /// What each tree's test printed in this session.
    outputs: HashMap<String, Kept>,
    /// The tree whose output shows.
    output_open: Option<String>,
    /// What the dialog offered last.
    pub(super) last: Option<StartTests>,
}

/// A run, as the banner shows it.
pub(super) struct LaneRun {
    id: u64,
    range: String,
    command: String,
    /// The commits it has to test (by tree), once listed.
    total: Option<usize>,
    queued: HashSet<String>,
    running: HashSet<String>,
    good: usize,
    bad: usize,
    failed: usize,
    /// The first commit that failed, oldest first.
    first_bad: Option<String>,
    stop: RunStop,
    started: Instant,
    ended: Option<Result<RunSummary, String>>,
}

impl LaneRun {
    fn going(&self) -> bool {
        self.ended.is_none()
    }
}

/// A run still going when its tab closes is stopped: nothing would show its end.
impl Drop for LaneRun {
    fn drop(&mut self) {
        if self.going() {
            self.stop.stop();
        }
    }
}

impl TestLane {
    /// The dot for `tree`, if there is one to show.
    pub(crate) fn dot(&self, tree: &str) -> Option<DotState> {
        if tree.is_empty() {
            return None;
        }
        if let Some(run) = self.run.as_ref().filter(|r| r.going()) {
            if run.running.contains(tree) {
                return Some(DotState::Running);
            }
            if run.queued.contains(tree) {
                return Some(DotState::Queued);
            }
        }
        match self.results.get(tree) {
            Some(Verdict::Good) => Some(DotState::Good),
            Some(Verdict::Bad) => Some(DotState::Bad),
            None => self
                .outputs
                .get(tree)
                .filter(|k| matches!(k.end, TestEnd::Failed(_)))
                .map(|_| DotState::Failed),
        }
    }

    /// Whether a run is going on.
    pub(crate) fn running(&self) -> bool {
        self.run.as_ref().is_some_and(LaneRun::going)
    }

    /// Whether the banner shows.
    pub(super) fn shows_banner(&self) -> bool {
        self.run.is_some() || (self.visible && self.error.is_some())
    }

    /// The repository's test command, if it has one.
    pub(crate) fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }
}

impl RepoTab {
    pub(super) fn test_lane_act(&mut self, act: LaneAct, ctx: &egui::Context) {
        match act {
            LaneAct::Start(start) => self.start_tests(start, ctx),
            LaneAct::Stop => {
                if let Some(run) = &self.test_lane.run {
                    run.stop.stop();
                }
            }
            LaneAct::Close => {
                if !self.test_lane.running() {
                    self.test_lane.run = None;
                    self.test_lane.error = None;
                }
            }
            LaneAct::ShowOutput(tree) => self.test_lane.output_open = Some(tree),
            LaneAct::CloseOutput => self.test_lane.output_open = None,
        }
    }

    /// Takes in whether the lane shows this frame: just shown, its results are read.
    pub(super) fn test_lane_frame(&mut self, settings: &Settings, ctx: &egui::Context) {
        let was = self.test_lane.visible;
        self.test_lane.visible = settings.show_test_lane;
        if self.test_lane.visible && !was {
            self.read_test_lane(ctx);
        }
    }

    /// Reads the test command and results again, in the background (after every reload, when
    /// the lane shows or a run is going on).
    pub(super) fn read_test_lane(&mut self, ctx: &egui::Context) {
        if !self.test_lane.visible && self.test_lane.run.is_none() {
            return;
        }
        self.test_lane.reads += 1;
        let read = self.test_lane.reads;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let command = git::test_command(&path, git::DEFAULT_TEST)?;
                let results = git::read_results(&path, git::DEFAULT_TEST)?;
                Ok((command, results))
            },
            move |tab, result: Result<_, String>, _| {
                let lane = &mut tab.test_lane;
                if read <= lane.shown {
                    return;
                }
                lane.shown = read;
                match result {
                    Ok((command, results)) => {
                        lane.command = command;
                        lane.results = results;
                        lane.error = None;
                    }
                    Err(e) => lane.error = Some(format!("Couldn't read the test results: {e}")),
                }
            },
        );
    }

    /// The dialog that starts a run, for the commits up to `tip` (the checked-out branch's
    /// when `None`).
    pub(super) fn test_dialog(&self, tip: Option<&str>) -> Action {
        let tip = tip
            .map(str::to_owned)
            .or_else(|| self.refs.current.clone())
            .unwrap_or_else(|| "HEAD".to_owned());
        let base = self.test_base();
        let range = match base {
            Some(base) if base != tip => format!("{base}..{tip}"),
            _ => tip,
        };
        let last = self.test_lane.last.clone();
        let command = self
            .test_lane
            .command
            .clone()
            .or_else(|| last.as_ref().map(|l| l.command.clone()))
            .unwrap_or_default();
        Action::Dialog(Dialog::TestCommits {
            start: StartTests {
                range,
                command,
                workers: last.as_ref().map_or(DEFAULT_WORKERS, |l| l.workers),
                retest: false,
                forget: true,
            },
            stored: self.test_lane.command.clone(),
            focus: true,
        })
    }

    /// Where a branch's commits to test begin: the branch pull requests go into.
    fn test_base(&self) -> Option<String> {
        self.base_branch_name()
    }

    fn start_tests(&mut self, start: StartTests, ctx: &egui::Context) {
        if self.test_lane.running() {
            self.status = Some((
                "Tests are running already: stop them first".to_owned(),
                true,
            ));
            return;
        }
        let command = start.command.trim().to_owned();
        if command.is_empty() {
            self.status = Some(("Type the command to test with".to_owned(), true));
            return;
        }
        self.test_lane.last = Some(start.clone());
        self.test_lane.runs += 1;
        let id = self.test_lane.runs;
        let stop = RunStop::default();
        self.test_lane.run = Some(LaneRun {
            id,
            range: start.range.trim().to_owned(),
            command: command.clone(),
            total: None,
            queued: HashSet::new(),
            running: HashSet::new(),
            good: 0,
            bad: 0,
            failed: 0,
            first_bad: None,
            stop: stop.clone(),
            started: Instant::now(),
            ended: None,
        });
        // The lane shows what the run finds.
        let mut settings = Settings::get(ctx);
        if !settings.show_test_lane {
            settings.show_test_lane = true;
            settings.set(ctx);
        }
        let stored = self.test_lane.command.clone();
        let path = self.path.clone();
        let tx = self.tx.clone();
        let wake = ctx.clone();
        let send = move |then: ToTab| {
            let _ = tx.send(Msg::Then(then));
            wake.request_repaint();
        };
        let failed = {
            let send = send.clone();
            move |e: String| send(Box::new(move |tab, ctx| tab.tests_ended(id, Err(e), ctx)))
        };
        std::thread::spawn(move || {
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_in_background(&path, &start, &command, stored.as_deref(), &stop, &send, id)
            }));
            match ran {
                Ok(result) => send(Box::new(move |tab, ctx| tab.tests_ended(id, result, ctx))),
                Err(_) => failed("Internal error: the test run stopped".to_owned()),
            }
        });
    }

    /// The commits of the run `id` are listed: they wait their turn.
    fn tests_listed(&mut self, id: u64, trees: Vec<String>) {
        let Some(run) = self.test_lane.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        run.total = Some(trees.len());
        run.queued = trees.into_iter().collect();
    }

    fn tests_event(&mut self, id: u64, event: TestEvent) {
        let lane = &mut self.test_lane;
        let Some(run) = lane.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        match event {
            TestEvent::Started { tree, .. } => {
                run.queued.remove(&tree);
                run.running.insert(tree);
            }
            TestEvent::Done {
                commit,
                tree,
                end,
                lines,
            } => {
                run.queued.remove(&tree);
                run.running.remove(&tree);
                match end.verdict() {
                    Some(Verdict::Good) => run.good += 1,
                    Some(Verdict::Bad) => {
                        run.bad += 1;
                        run.first_bad.get_or_insert_with(|| commit.clone());
                    }
                    None => run.failed += 1,
                }
                match end.verdict() {
                    Some(verdict) => {
                        lane.results.insert(tree.clone(), verdict);
                    }
                    None => {
                        lane.results.remove(&tree);
                    }
                }
                // A stored result says nothing new: what ran before (if anything) stays.
                if !matches!(end, TestEnd::Known(_)) {
                    lane.outputs.insert(tree, Kept { commit, end, lines });
                }
            }
        }
    }

    fn tests_ended(&mut self, id: u64, result: Result<RunSummary, String>, ctx: &egui::Context) {
        let name = self.name.clone();
        let Some(run) = self.test_lane.run.as_mut().filter(|r| r.id == id) else {
            return;
        };
        run.queued.clear();
        run.running.clear();
        let (message, failed) = ended_message(&run.range, &result);
        if run.started.elapsed() > LONG_RUN {
            self.finished.push((format!("{name} · {message}"), failed));
        }
        run.ended = Some(result);
        self.status = Some((message, failed));
        // What the run stored, and what the CLI may have meanwhile.
        self.read_test_lane(ctx);
    }

    /// The palette's entries for the lane.
    pub(super) fn test_palette_items(&self) -> Vec<(String, String, Action)> {
        let item = |label: &str, action| (label.to_owned(), "tests test lane".to_owned(), action);
        let mut items = Vec::new();
        if self.test_lane.running() {
            items.push(item("Stop testing", Action::TestLane(LaneAct::Stop)));
        } else if !self.refs.head.is_empty() {
            items.push(item("Test commits…", self.test_dialog(None)));
        }
        items
    }

    /// The commit menu's entry: test the commits up to `hash`.
    pub(super) fn test_menu_items(&self, ui: &mut egui::Ui, hash: &str, acts: &mut Vec<Action>) {
        let running = self.test_lane.running();
        if ui
            .add_enabled(!running, egui::Button::new("Test commits up to here…"))
            .on_hover_text(
                "Runs a test command at each commit from the base branch to this one, in \
                 scratch worktrees, and shows a pass / fail dot by each",
            )
            .on_disabled_hover_text("Tests are running: stop them first")
            .clicked()
        {
            acts.push(self.test_dialog(Some(hash)));
            ui.close();
        }
    }
}

/// What a run's thread leaves the tab to do with what it found.
type ToTab = super::messages::Continuation;

/// What a run's thread does: stores the command, lists the commits, tests them. `send` takes
/// what the tab is to do with what it found.
fn run_in_background(
    path: &std::path::Path,
    start: &StartTests,
    command: &str,
    stored: Option<&str>,
    stop: &RunStop,
    send: &(impl Fn(ToTab) + Sync),
    id: u64,
) -> Result<RunSummary, String> {
    if stored != Some(command) {
        if start.forget && stored.is_some() {
            git::forget_results(path, git::DEFAULT_TEST)?;
        }
        git::set_test_command(path, git::DEFAULT_TEST, command)?;
    }
    let jobs = git::commits_to_test(path, &start.range)?;
    let trees: Vec<String> = jobs.iter().map(|j| j.tree.clone()).collect();
    send(Box::new(move |tab, _| tab.tests_listed(id, trees)));
    let plan = TestPlan {
        name: git::DEFAULT_TEST.to_owned(),
        command: command.to_owned(),
        workers: start.workers,
        retest: start.retest,
    };
    let tell = |event: TestEvent| send(Box::new(move |tab, _| tab.tests_event(id, event)));
    git::run_tests(path, jobs, &plan, stop, &tell)
}

/// What the status line says when the run of `range` ended so, and whether it is bad news.
pub(crate) fn ended_message(range: &str, result: &Result<RunSummary, String>) -> (String, bool) {
    match result {
        Err(e) => (format!("Testing {range} failed: {e}"), true),
        Ok(s) => {
            let tested = s.good + s.bad;
            let mut parts = vec![format!("{} passed", s.good)];
            if s.bad > 0 {
                parts.push(format!("{} failed", s.bad));
            }
            if !s.failed.is_empty() {
                parts.push(format!("{} couldn't be tested", s.failed.len()));
            }
            let noun = if tested == 1 { "commit" } else { "commits" };
            let head = if s.stopped {
                format!("Testing {range} stopped after {tested} {noun}")
            } else {
                format!("Tested {tested} {noun} of {range}")
            };
            let bad = s.bad > 0 || !s.failed.is_empty();
            (format!("{head}: {}", parts.join(", ")), bad)
        }
    }
}

/// How the dot of `state` reads on hover, for the tree `tree` and the test `command`.
pub(crate) fn dot_tip(state: DotState, command: Option<&str>, tree: &str, ran: bool) -> String {
    let cmd = command.map_or_else(|| "the test".to_owned(), |c| format!("`{c}`"));
    let what = match state {
        DotState::Good => format!("Passed {cmd}"),
        DotState::Bad => format!("Failed {cmd}"),
        DotState::Running => format!("Running {cmd}…"),
        DotState::Queued => "Waiting to be tested".to_owned(),
        DotState::Failed => "Couldn't be tested".to_owned(),
    };
    let how = match state {
        DotState::Good | DotState::Bad if ran => "\nClick to see what it printed",
        DotState::Failed => "\nClick to see why",
        DotState::Good | DotState::Bad => {
            "\nStored result (kept by tree in refs/notes/tests/default, shared with git-test)"
        }
        _ => "",
    };
    format!("{what} · tree {}{how}", short(tree))
}
