//! Parallel bisect from the tab: started from the bisect banner (its good and bad marks), a
//! commit's menu or the palette; then followed as its events come in, which the banner and the
//! graph rows are drawn from. The search itself runs in `crate::parallel_bisect`, off this
//! thread.

mod banner;
mod items;
mod marks;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Action, Dialog, RepoTab, short};
use crate::parallel_bisect::{self as pb, Event, Handle, Machine, Outcome, TreeCache, Verdict};

pub(super) use crate::repo::dialogs::ParallelForm;

/// A bisect that takes longer than this gets a notification when it ends.
const LONG_RUN: Duration = Duration::from_secs(4);

/// What the views ask of a parallel bisect.
pub(crate) enum ParallelAct {
    /// Open the dialog that starts one, with these ends (`good` may be empty).
    Open {
        bad: String,
        good: Vec<String>,
        skip: Vec<String>,
    },
    /// Start one.
    Start(Box<pb::Setup>),
    /// Stop every test and end it.
    Cancel,
    /// Let go of what one that ended found.
    Close,
    /// Mark what it found in the bisect going on (`git bisect`), so that it ends there too.
    MarkInBisect,
}

/// The tab's parallel bisect: the one running, or the last one, until closed.
#[derive(Default)]
pub(super) struct Parallel {
    handle: Option<Handle>,
    view: Option<View>,
    /// Answers by tree, for as long as the tab is open.
    cache: TreeCache,
    /// What the dialog offers next time: the last command, machine, parallelism, timeout.
    last: Option<pb::Setup>,
    /// What a crashed bisect left was looked for, when the tab first read the repository.
    leftovers_cleared: bool,
}

/// What the banner and the graph show of a parallel bisect.
pub(super) struct View {
    machine: Machine,
    parallel: usize,
    /// What it is doing, before the rounds (or after them, while clearing up).
    step: Option<String>,
    round: usize,
    rounds_left: usize,
    /// How many commits it began with, and those that may still be the first bad one.
    total: usize,
    remaining: Arc<HashSet<String>>,
    testing: HashSet<String>,
    results: HashMap<String, Tested>,
    tests_run: usize,
    outcome: Option<Outcome>,
    warning: Option<String>,
    started: Instant,
    /// Cancel was asked for.
    cancelling: bool,
    /// Its ends as given: whether what it found answers the bisect going on.
    bad: String,
    good: Vec<String>,
}

/// What the test said of a commit.
#[derive(Clone)]
pub(super) struct Tested {
    verdict: Verdict,
    /// It wasn't tested itself: a commit with its tree was.
    cached: bool,
    output: String,
}

impl View {
    fn machine_label(&self) -> String {
        match &self.machine {
            Machine::Here => crate::i18n::text("repo-workflows-parallel-machine-here").to_owned(),
            Machine::There(_) => self.machine.label(),
        }
    }

    fn new(setup: &pb::Setup) -> Self {
        Self {
            machine: setup.machine.clone(),
            parallel: setup.parallel,
            step: None,
            round: 0,
            rounds_left: 0,
            total: 0,
            remaining: Arc::default(),
            testing: HashSet::new(),
            results: HashMap::new(),
            tests_run: 0,
            outcome: None,
            warning: None,
            started: Instant::now(),
            cancelling: false,
            bad: setup.bad.clone(),
            good: setup.good.clone(),
        }
    }

    /// Takes in what the bisect said.
    fn take(&mut self, event: Event) {
        match event {
            // Once cancelled, it says it is stopping until it has.
            Event::Step(_) if self.cancelling => {}
            Event::Step(step) => self.step = Some(step),
            Event::Remaining(left) => {
                if self.total == 0 {
                    self.total = left.len();
                }
                self.remaining = left;
            }
            Event::Round {
                number,
                rounds_left,
                testing,
            } => {
                self.step = None;
                self.round = number;
                self.rounds_left = rounds_left;
                self.testing = testing.into_iter().collect();
            }
            Event::Tested {
                hash,
                verdict,
                cached,
                output,
                same_tree,
            } => {
                self.testing.remove(&hash);
                if !cached {
                    self.tests_run += 1;
                }
                let tested = Tested {
                    verdict,
                    cached,
                    output,
                };
                for other in same_tree.into_iter().filter(|h| *h != hash) {
                    let copy = Tested {
                        cached: true,
                        ..tested.clone()
                    };
                    self.results.entry(other).or_insert(copy);
                }
                self.results.insert(hash, tested);
            }
            Event::Dropped(hash) => {
                self.testing.remove(&hash);
            }
            Event::Warning(warning) => self.warning = Some(warning),
            Event::Done(outcome) => {
                self.step = None;
                self.testing.clear();
                self.outcome = Some(outcome);
            }
        }
    }

    pub(super) fn running(&self) -> bool {
        self.outcome.is_none()
    }

    /// Whether it bisected between the marks of `bisect` (its first bad commit then answers
    /// that bisect too).
    pub(super) fn same_ends(&self, bisect: &crate::git::Bisect) -> bool {
        let mut good = self.good.clone();
        let mut marked = bisect.marks.good.clone();
        good.sort();
        marked.sort();
        bisect.marks.bad.as_deref() == Some(self.bad.as_str()) && good == marked
    }

    /// The first bad commit, or those it may be, once found.
    pub(super) fn culprits(&self) -> &[String] {
        match &self.outcome {
            Some(Outcome::FirstBad(hash)) => std::slice::from_ref(hash),
            Some(Outcome::OneOf(hashes)) => hashes,
            _ => &[],
        }
    }
}

impl Parallel {
    /// The bisect shown (running, or ended and not closed yet).
    pub(super) fn view(&self) -> Option<&View> {
        self.view.as_ref()
    }

    pub(super) fn running(&self) -> bool {
        self.view.as_ref().is_some_and(View::running)
    }
}

impl RepoTab {
    pub(super) fn parallel_act(&mut self, act: ParallelAct, ctx: &egui::Context) {
        match act {
            ParallelAct::Open { bad, good, skip } => {
                // After one that failed (a commit mistyped…), what was typed is offered again.
                let failed = self
                    .parallel
                    .view()
                    .is_some_and(|v| matches!(v.outcome, Some(Outcome::Failed(_))));
                let (bad, good) = match (&self.parallel.last, failed) {
                    (Some(last), true) => (last.bad.clone(), last.good.clone()),
                    _ => (bad, good),
                };
                let form =
                    ParallelForm::new(&self.path, bad, good, skip, self.parallel.last.as_ref());
                self.dialog = Some(Dialog::ParallelBisect(Box::new(form)));
            }
            ParallelAct::Start(setup) => self.start_parallel(*setup, ctx),
            ParallelAct::Cancel => {
                if let Some(handle) = &self.parallel.handle {
                    handle.cancel();
                }
                if let Some(view) = &mut self.parallel.view
                    && view.running()
                {
                    view.cancelling = true;
                    view.step = Some(
                        crate::i18n::text("repo-workflows-parallel_bisect-parallel-act-b0e5ccdc")
                            .to_owned(),
                    );
                }
            }
            ParallelAct::Close => {
                if !self.parallel.running() {
                    self.parallel.view = None;
                    self.parallel.handle = None;
                }
            }
            ParallelAct::MarkInBisect => self.mark_parallel_result(ctx),
        }
    }

    fn start_parallel(&mut self, setup: pb::Setup, ctx: &egui::Context) {
        if self.parallel.running() {
            self.status = Some((
                "A parallel bisect is already running in this tab".to_owned(),
                true,
            ));
            return;
        }
        self.status = None;
        self.parallel.view = Some(View::new(&setup));
        self.parallel.last = Some(setup.clone());
        let repaint = ctx.clone();
        let handle = pb::start(setup, self.parallel.cache.clone(), move || {
            repaint.request_repaint();
        });
        self.parallel.handle = Some(handle);
    }

    /// Takes in what the bisect running said since the last frame.
    pub(super) fn poll_parallel(&mut self, ctx: &egui::Context) {
        let Some(handle) = &self.parallel.handle else {
            return;
        };
        let events = handle.events();
        let Some(view) = &mut self.parallel.view else {
            return;
        };
        let mut ended = None;
        for event in events {
            if let Event::Done(outcome) = &event {
                ended = Some(outcome.clone());
            }
            view.take(event);
        }
        let Some(outcome) = ended else {
            return;
        };
        self.parallel.handle = None;
        let long = view.started.elapsed() > LONG_RUN;
        let (message, failed) = outcome_message(&outcome, &view.machine_label());
        if long {
            self.finished.push((
                crate::i18n::message(
                    "repo-workflows-parallel_bisect-poll-parallel-e34dd853",
                    &[
                        ("self-name-1", (self.name).clone().into()),
                        ("message", (message).clone().into()),
                    ],
                ),
                failed,
            ));
        }
        self.status = Some((message, failed));
        if let Outcome::FirstBad(hash) = &outcome {
            self.goto(&hash.clone(), ctx);
        }
    }

    /// The parallel bisect's answer for `hash`, if it has one.
    pub(super) fn parallel_result(&self, hash: &str) -> Option<&Tested> {
        self.parallel.view()?.results.get(hash)
    }

    /// Marks the first bad commit found in the bisect going on: it bad, its parents good. git
    /// then ends there with the same answer, and Finish puts the checkout back.
    fn mark_parallel_result(&mut self, ctx: &egui::Context) {
        let Some(Outcome::FirstBad(hash)) = self.parallel.view().and_then(|v| v.outcome.clone())
        else {
            return;
        };
        let Some(bisect) = self.bisect.state().cloned() else {
            return;
        };
        if self.busy_op.is_some() {
            self.status = Some(("Wait for the operation going on to finish".to_owned(), true));
            return;
        }
        let parents = self
            .index
            .get(&hash)
            .and_then(|&i| self.commits.get(i))
            .map(|c| c.parents.clone());
        let path = self.path.clone();
        self.busy_op = Some("Mark in the bisect".to_owned());
        self.op_started = Some(Instant::now());
        self.then(
            ctx,
            move || {
                let parents = match parents {
                    Some(parents) => parents,
                    None => crate::git::commit_detail(&path, &hash)?.parents,
                };
                for parent in &parents {
                    crate::git::bisect_mark(&path, &bisect, crate::git::Mark::Good, Some(parent))?;
                }
                crate::git::bisect_mark(&path, &bisect, crate::git::Mark::Bad, Some(&hash))
            },
            |tab, result, ctx| {
                tab.busy_op = None;
                tab.op_started = None;
                tab.status = Some(match result {
                    Ok(_) => ("Marked in the bisect".to_owned(), false),
                    Err(e) => (format!("Mark in the bisect failed: {e}"), true),
                });
                tab.reload(ctx);
            },
        );
    }

    /// Opens the dialog for a parallel bisect from the commit `hash`'s menu: it is the good end
    /// and HEAD the bad one, unless it is HEAD.
    pub(super) fn parallel_from(&self, hash: &str) -> Action {
        let (bad, good) = if hash == self.refs.head {
            (hash.to_owned(), Vec::new())
        } else {
            ("HEAD".to_owned(), vec![hash.to_owned()])
        };
        Action::ParallelBisect(ParallelAct::Open {
            bad,
            good,
            skip: Vec::new(),
        })
    }

    /// Opens the dialog with the marks of the bisect going on.
    pub(super) fn parallel_from_bisect(&self) -> Option<Action> {
        let bisect = self.bisect.state()?;
        let bad = bisect.marks.bad.clone()?;
        Some(Action::ParallelBisect(ParallelAct::Open {
            bad,
            good: bisect.marks.good.clone(),
            skip: bisect.marks.skipped.clone(),
        }))
    }
}

/// How a parallel bisect ended, for the status line: the message, and whether it failed.
fn outcome_message(outcome: &Outcome, machine: &str) -> (String, bool) {
    match outcome {
        Outcome::FirstBad(hash) => (
            crate::i18n::message(
                "repo-workflows-parallel_bisect-outcome-message-323d18af",
                &[("short-hash-1", (short(hash)).clone().into())],
            ),
            false,
        ),
        Outcome::OneOf(hashes) => (
            crate::i18n::message(
                "repo-workflows-parallel_bisect-outcome-message-5f365ac7",
                &[("hashes-len-1", (hashes.len()).clone().into())],
            ),
            false,
        ),
        Outcome::GoodEndFails { hash, .. } => (
            crate::i18n::message(
                "repo-workflows-parallel_bisect-outcome-message-a5f8e4f1",
                &[
                    ("short-hash-1", (short(hash)).clone().into()),
                    ("machine", (machine).clone().into()),
                ],
            ),
            true,
        ),
        Outcome::BadEndPasses { hash, .. } => (
            crate::i18n::message(
                "repo-workflows-parallel_bisect-outcome-message-12d782ed",
                &[
                    ("short-hash-1", (short(hash)).clone().into()),
                    ("machine", (machine).clone().into()),
                ],
            ),
            true,
        ),
        Outcome::Cancelled => (
            crate::i18n::text("repo-workflows-parallel_bisect-outcome-message-9edd35e1").to_owned(),
            false,
        ),
        // Its first line: what follows (a test's output) shows on the banner's hover.
        Outcome::Failed(e) => (
            crate::i18n::message(
                "repo-workflows-parallel_bisect-outcome-message-db128bb5",
                &[(
                    "e-lines-next-unwrap-or-default-1",
                    (e.lines().next().unwrap_or_default()).clone().into(),
                )],
            ),
            true,
        ),
    }
}

/// The machines a bisect can run on, for the repository at `repo`: its own first, then this
/// computer, the SSH hosts of `~/.ssh/config` (WSL distributions come later, looked up in the
/// background).
pub(super) fn machines(repo: &std::path::Path) -> Vec<Machine> {
    let mut list = vec![Machine::of(repo)];
    let mut add = |m: Machine| {
        if !list.contains(&m) {
            list.push(m);
        }
    };
    add(Machine::Here);
    for host in crate::remote::ssh_config_hosts() {
        add(Machine::There(crate::remote::Host::Ssh(host)));
    }
    list
}
