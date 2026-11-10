//! One parallel bisect, on a thread of its own: it gets the machine ready, reads the range,
//! then tests k commits at a time (one per worktree, side by side) until the first bad commit
//! is known, telling the tab what happens as it happens. Whatever way it ends, its worktrees
//! are removed.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::cache::TreeCache;
use super::dag::Dag;
use super::machine::Machine;
use super::runner::Ran;
use super::search::{Conclusion, Search};
use super::verdict::{Ended, Verdict, interpret};
use super::workspace::Workspace;
use super::{MAX_PARALLEL, short};
use crate::git;

type SessionThread = (std::sync::Weak<AtomicBool>, std::thread::JoinHandle<()>);
static SESSIONS: Mutex<Vec<SessionThread>> = Mutex::new(Vec::new());

pub(crate) fn shutdown() {
    let sessions = std::mem::take(&mut *SESSIONS.lock().unwrap_or_else(PoisonError::into_inner));
    for (stop, _) in &sessions {
        if let Some(stop) = stop.upgrade() {
            stop.store(true, Ordering::Relaxed);
        }
    }
    for (_, thread) in sessions {
        let _ = thread.join();
    }
}

/// What to bisect, with what, and where.
#[derive(Clone, Debug)]
pub struct Setup {
    /// The tab's repository, which has the commits.
    pub repo: PathBuf,
    /// The bad commit and the good ones (hashes, branches, tags…).
    pub bad: String,
    pub good: Vec<String>,
    /// Commits already known to be untestable (git's own bisect's skips).
    pub skip: Vec<String>,
    /// What tests a commit, as `git bisect run` takes it (run with `sh -c` at the top of the
    /// worktree): 0 good, 125 skip, 1–127 otherwise bad.
    pub command: String,
    pub machine: Machine,
    /// A repository on `machine` to work in instead of the tab's own (or of a copy made there):
    /// a path there.
    pub repo_there: Option<String>,
    /// How many commits are tested at once.
    pub parallel: usize,
    /// How long one test may run; past it, it is stopped and counts as skipped.
    pub timeout: Duration,
}

/// What the bisect tells the tab, as it happens.
#[derive(Clone, Debug)]
pub enum Event {
    /// What it is doing (before the tests start, and once they are done).
    Step(String),
    /// The commits that may still be the first bad one (first: the whole range).
    Remaining(Arc<HashSet<String>>),
    /// A round of tests started: its number, about how many more after it, its commits.
    Round {
        number: usize,
        rounds_left: usize,
        testing: Vec<String>,
    },
    /// A commit was tested (or its tree was before: `cached`). `same_tree`: the commits of the
    /// range that got the same answer for having the same tree, `hash` among them when it is
    /// in the range.
    Tested {
        hash: String,
        verdict: Verdict,
        cached: bool,
        output: String,
        same_tree: Vec<String>,
    },
    /// The test of a commit was stopped: its answer was no longer needed.
    Dropped(String),
    /// Something went wrong that doesn't change the answer (the clean-up).
    Warning(String),
    Done(Outcome),
    /// Opt-in paint receipt beside the unchanged raw outcome.
    DoneWithPresentation(Outcome, Option<super::presentation::Presentation>),
    WarningWithPresentation(String, super::presentation::CleanupWarning),
}

/// How a bisect ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    FirstBad(String),
    /// Skipped commits left a choice: one of these (the bad one first).
    OneOf(Vec<String>),
    /// The commit marked good fails the test: there is nothing to look for between them.
    GoodEndFails {
        hash: String,
        output: String,
    },
    /// The commit marked bad passes the test: the test doesn't show the problem.
    BadEndPasses {
        hash: String,
        output: String,
    },
    Cancelled,
    Failed(String),
}

/// The tab's hold on a running bisect. Dropped, it cancels it (the clean-up still runs).
pub struct Handle {
    events: Receiver<Event>,
    stop: Arc<AtomicBool>,
}

impl Handle {
    /// What happened since last asked.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Stops every test that runs and ends the bisect (it then says `Done(Cancelled)`).
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Starts bisecting as `setup` says, results kept in `cache`; `wake` is called after each
/// event (to have the window drawn again).
#[cfg(test)]
pub fn start(setup: Setup, cache: TreeCache, wake: impl Fn() + Send + Sync + 'static) -> Handle {
    start_impl(setup, cache, wake, false)
}
pub(crate) fn start_with_presentation(
    setup: Setup,
    cache: TreeCache,
    wake: impl Fn() + Send + Sync + 'static,
) -> Handle {
    start_impl(setup, cache, wake, true)
}
fn start_impl(
    setup: Setup,
    cache: TreeCache,
    wake: impl Fn() + Send + Sync + 'static,
    with_presentation: bool,
) -> Handle {
    let (tx, events) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = Handle {
        events,
        stop: Arc::clone(&stop),
    };
    let weak_stop = Arc::downgrade(&stop);
    let thread = std::thread::spawn(move || {
        let out = Out {
            tx: Mutex::new(tx),
            wake: Box::new(wake),
            with_presentation,
        };
        let mut presentation = None;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bisect(&setup, &cache, &out, &stop, &mut presentation)
        }))
        .unwrap_or_else(|_| {
            presentation = None;
            Outcome::Failed("the bisect stopped on an error in gitcraft".into())
        });
        out.send(if with_presentation {
            done_with_presentation(outcome, presentation)
        } else {
            Event::Done(outcome)
        });
    });
    let mut sessions = SESSIONS.lock().unwrap_or_else(PoisonError::into_inner);
    sessions.retain(|(_, thread)| !thread.is_finished());
    sessions.push((weak_stop, thread));
    handle
}

/// Where events go.
struct Out {
    tx: Mutex<Sender<Event>>,
    wake: Box<dyn Fn() + Send + Sync>,
    with_presentation: bool,
}

impl Out {
    fn send(&self, event: Event) {
        let tx = self.tx.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = tx.send(event);
        (self.wake)();
    }

    fn step(&self, what: impl Into<String>) {
        self.send(Event::Step(what.into()));
    }
}

fn bisect(
    setup: &Setup,
    cache: &TreeCache,
    out: &Out,
    stop: &AtomicBool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Outcome {
    *presentation = None;
    let setup = match resolved(setup).and_then(|s| check(&s).map(|()| s)) {
        Ok(setup) => setup,
        Err(e) => return Outcome::Failed(e),
    };
    prepared_outcome(prepared(&setup, cache, out, stop, presentation))
}
fn prepared_outcome(result: Result<Outcome, String>) -> Outcome {
    // Errors can contain unknown ownership/cleanup. A late stop flag cannot prove
    // confirmed cancellation; that arrives explicitly as Ok(Outcome::Cancelled).
    result.unwrap_or_else(Outcome::Failed)
}
fn record_round_result(
    ended: &mut Option<Result<Outcome, String>>,
    result: Result<Ran, String>,
    cancelled: bool,
    answer: impl FnOnce(Ran) -> Result<Option<Outcome>, String>,
) -> Option<String> {
    let ran = match result {
        Ok(ran) => ran,
        Err(error) => {
            if ended.is_some() {
                return Some(error);
            }
            *ended = Some(Err(error));
            return None;
        }
    };
    if ended.is_some() || cancelled {
        return None;
    }
    match answer(ran) {
        Err(error) => *ended = Some(Err(error)),
        Ok(Some(outcome)) => *ended = Some(Ok(outcome)),
        Ok(None) => {}
    }
    None
}
fn finish_round(
    ended: Option<Result<Outcome, String>>,
    cancelled: bool,
) -> Result<Option<Outcome>, String> {
    match ended {
        Some(result) => result.map(Some),
        None if cancelled => Ok(Some(Outcome::Cancelled)),
        None => Ok(None),
    }
}

/// `setup` with its commits as full hashes (they may be typed as branches, tags, `HEAD~3`).
fn resolved(setup: &Setup) -> Result<Setup, String> {
    let resolve = |rev: &String| git::resolve(&setup.repo, rev.trim());
    Ok(Setup {
        bad: resolve(&setup.bad)?,
        good: setup.good.iter().map(resolve).collect::<Result<_, _>>()?,
        skip: setup.skip.iter().map(resolve).collect::<Result<_, _>>()?,
        ..setup.clone()
    })
}

/// What can't be bisected at all.
fn check(setup: &Setup) -> Result<(), String> {
    if setup.command.trim().is_empty() {
        return Err("type the command that tests a commit".to_owned());
    }
    if setup.good.is_empty() {
        return Err("mark at least one good commit".to_owned());
    }
    let hex = |h: &String| h.len() >= 40 && h.chars().all(|c| c.is_ascii_hexdigit());
    if !hex(&setup.bad) || !setup.good.iter().all(hex) || !setup.skip.iter().all(hex) {
        return Err("the good and bad commits have to be full commit hashes".to_owned());
    }
    if setup.good.contains(&setup.bad) {
        return Err("the same commit is marked good and bad".to_owned());
    }
    Ok(())
}

/// Gets the machine ready, bisects, and clears what it made there.
fn prepared(
    setup: &Setup,
    cache: &TreeCache,
    out: &Out,
    stop: &AtomicBool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<Outcome, String> {
    let label = setup.machine.label();
    out.step(format!("Setting up on {label}…"));
    let mut ws = Workspace::open(&setup.repo, &setup.machine, setup.repo_there.as_deref())?;
    let result = run_in(&ws, setup, cache, out, stop, presentation);
    out.step("Removing the worktrees…");
    let mut cleanup_presentation = None;
    if let Err(e) = ws.close_with_presentation(&mut cleanup_presentation) {
        out.send(cleanup_warning(
            label,
            e,
            cleanup_presentation,
            out.with_presentation,
        ));
    }
    super::presentation::retain_error(&result, presentation);
    result
}

fn run_in(
    ws: &Workspace,
    setup: &Setup,
    cache: &TreeCache,
    out: &Out,
    stop: &AtomicBool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<Outcome, String> {
    let mut wanted = vec![setup.bad.clone()];
    wanted.extend(setup.good.iter().cloned());
    if !crate::remote::same_dir(&setup.repo, &ws.repo) {
        out.step(format!("Sending the commits to {}…", ws.machine.label()));
    }
    ws.send_with_presentation(&setup.repo, &wanted, presentation)?;
    *presentation = None;
    out.step("Reading the commits to bisect…");
    let dag = load_range(setup)?;
    let mut search = Search::new(dag, &setup.bad)?;
    for hash in &setup.skip {
        if let Some(i) = search.dag().index_of(hash) {
            search.mark(i, Verdict::Skip);
        }
    }
    let ends = ends(setup, &search)?;
    let parallel = setup.parallel.clamp(1, MAX_PARALLEL);
    let mut run = Run {
        setup,
        ws,
        cache,
        out,
        stop,
        search,
        slots: (1..=parallel).map(|n| Slot::new(ws, n)).collect(),
        machine: setup.machine.label(),
    };
    run.send_remaining();
    run.rounds(ends)
}

/// The range from git: the commits the bad one reaches and no good one does.
fn load_range(setup: &Setup) -> Result<Dag, String> {
    let mut args = vec![
        "log".to_owned(),
        "--topo-order".to_owned(),
        "--no-show-signature".to_owned(),
        "--format=%H %T %P".to_owned(),
        setup.bad.clone(),
    ];
    args.extend(setup.good.iter().map(|g| format!("^{g}")));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    Dag::parse(&git::run(Some(&setup.repo), &args)?)
}

mod runner;
use runner::*;

pub(crate) fn done_with_presentation(
    outcome: Outcome,
    presentation: Option<super::presentation::Presentation>,
) -> Event {
    let presentation = match &outcome {
        Outcome::Failed(raw) => presentation.and_then(|p| p.matching(raw)),
        _ => None,
    };
    Event::DoneWithPresentation(outcome, presentation)
}
pub(crate) fn cleanup_warning(
    label: String,
    e: String,
    child: Option<super::presentation::Presentation>,
    with_presentation: bool,
) -> Event {
    let raw = format!(
        "The worktrees on {label} couldn't all be removed ({e}); recovery evidence was retained. \
         Resolve the reported ownership or removal failure before trying again."
    );
    if with_presentation {
        let presentation =
            super::presentation::CleanupWarning::capture(raw.clone(), label, e, child);
        Event::WarningWithPresentation(raw, presentation)
    } else {
        Event::Warning(raw)
    }
}

#[cfg(test)]
mod recording_outcome_tests;
