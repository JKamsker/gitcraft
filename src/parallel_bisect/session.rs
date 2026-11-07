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
use super::runner::{self, Ran};
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
pub fn start(setup: Setup, cache: TreeCache, wake: impl Fn() + Send + Sync + 'static) -> Handle {
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
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bisect(&setup, &cache, &out, &stop)
        }))
        .unwrap_or_else(|_| Outcome::Failed("the bisect stopped on an error in gitcraft".into()));
        out.send(Event::Done(outcome));
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

fn bisect(setup: &Setup, cache: &TreeCache, out: &Out, stop: &AtomicBool) -> Outcome {
    let setup = match resolved(setup).and_then(|s| check(&s).map(|()| s)) {
        Ok(setup) => setup,
        Err(e) => return Outcome::Failed(e),
    };
    match prepared(&setup, cache, out, stop) {
        Ok(outcome) => outcome,
        Err(_) if stop.load(Ordering::Relaxed) => Outcome::Cancelled,
        Err(e) => Outcome::Failed(e),
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
) -> Result<Outcome, String> {
    let label = setup.machine.label();
    out.step(format!("Setting up on {label}…"));
    let mut ws = Workspace::open(&setup.repo, &setup.machine, setup.repo_there.as_deref())?;
    let result = run_in(&ws, setup, cache, out, stop);
    out.step("Removing the worktrees…");
    if let Err(e) = ws.close() {
        out.send(Event::Warning(format!(
            "The worktrees on {label} couldn't all be removed ({e}); the next parallel bisect \
             there removes them."
        )));
    }
    result
}

fn run_in(
    ws: &Workspace,
    setup: &Setup,
    cache: &TreeCache,
    out: &Out,
    stop: &AtomicBool,
) -> Result<Outcome, String> {
    let mut wanted = vec![setup.bad.clone()];
    wanted.extend(setup.good.iter().cloned());
    if !crate::remote::same_dir(&setup.repo, &ws.repo) {
        out.step(format!("Sending the commits to {}…", ws.machine.label()));
    }
    ws.send(&setup.repo, &wanted)?;
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

/// A commit tested for the search, or one of its ends, tested first to make sure the test
/// tells them apart.
#[derive(Clone, Debug)]
struct Job {
    hash: String,
    tree: String,
    /// For an end: what it was marked.
    end: Option<Verdict>,
}

/// The ends to check: the bad commit and the good ones, with their trees.
fn ends(setup: &Setup, search: &Search) -> Result<Vec<Job>, String> {
    let mut ends = Vec::new();
    for (hash, marked) in std::iter::once((&setup.bad, Verdict::Bad))
        .chain(setup.good.iter().map(|g| (g, Verdict::Good)))
    {
        let tree = match search.dag().index_of(hash) {
            Some(i) => search.dag().tree(i).to_owned(),
            None => git::run(
                Some(&setup.repo),
                &["rev-parse", &format!("{hash}^{{tree}}")],
            )?
            .trim()
            .to_owned(),
        };
        ends.push(Job {
            hash: hash.clone(),
            tree,
            end: Some(marked),
        });
    }
    Ok(ends)
}

/// A worktree on the machine, made when first needed.
struct Slot {
    n: usize,
    path: PathBuf,
    there: String,
    made: bool,
}

impl Slot {
    fn new(ws: &Workspace, n: usize) -> Self {
        let (path, there) = ws.slot(n);
        Slot {
            n,
            path,
            there,
            made: false,
        }
    }

    /// Puts the worktree on `hash`, as it is in the commit (what an earlier test left is
    /// removed; ignored files, a build's output, stay). A new worktree is added one at a time
    /// (git reads the others' files as it adds one), then checked out side by side.
    fn checkout(&mut self, ws: &Workspace, hash: &str, limit: Duration) -> Result<(), String> {
        let failed = |exit: git::Exit| {
            let said = String::from_utf8_lossy(&exit.stderr).trim().to_owned();
            format!(
                "could not check out {} in a worktree on {}: {said}",
                short(hash),
                ws.machine.label()
            )
        };
        if !self.made {
            let add = [
                "worktree",
                "add",
                "-q",
                "--no-checkout",
                "--detach",
                "--force",
                &self.there,
                hash,
            ];
            let exit = ws.adding(|| git::run_for_test(&ws.repo, &add, limit))?;
            if exit.code != Some(0) {
                return Err(failed(exit));
            }
            self.made = true;
        }
        for args in [
            &["checkout", "-q", "--detach", "--force", hash][..],
            &["clean", "-q", "-ffd"],
        ] {
            let exit = git::run_for_test(&self.path, args, limit)?;
            if exit.code != Some(0) {
                return Err(failed(exit));
            }
        }
        Ok(())
    }

    /// Checks `hash` out and runs the test there.
    fn test(
        &mut self,
        ws: &Workspace,
        hash: &str,
        setup: &Setup,
        stop: &(dyn Fn() -> bool + Sync),
    ) -> Result<Ran, String> {
        self.checkout(ws, hash, setup.timeout.max(Duration::from_secs(600)))?;
        if stop() {
            return Ok(Ran {
                ended: Ended::Stopped,
                output: String::new(),
            });
        }
        let mut random = [0u8; 16];
        gitcraft_os::random_bytes(&mut random).map_err(|e| e.to_string())?;
        let unique: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let owner_record = crate::remote::join(&ws.base, &format!("{}-{unique}.owner", self.n));
        let log = format!("{}/{}.log", ws.base_there, self.n);
        runner::run(
            &runner::Test {
                dir: &self.path,
                control: &ws.repo,
                owner_record: &owner_record,
                log: &log,
                command: &setup.command,
                limit: setup.timeout,
            },
            stop,
        )
    }
}

/// The rounds of a bisect, and what it knows so far.
struct Run<'a> {
    setup: &'a Setup,
    ws: &'a Workspace,
    cache: &'a TreeCache,
    out: &'a Out,
    stop: &'a AtomicBool,
    search: Search,
    slots: Vec<Slot>,
    /// The machine, as the cache knows it.
    machine: String,
}

impl Run<'_> {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Round after round until the answer is known: the ends first (with as many commits as
    /// there is room for beside them), then k commits at a time.
    fn rounds(&mut self, mut ends: Vec<Job>) -> Result<Outcome, String> {
        // Ends tested before need no test now.
        let mut untested = Vec::new();
        for end in ends.drain(..) {
            match self.cached(&end.tree) {
                Some(verdict) => {
                    if let Some(outcome) = self.take(&end, verdict, String::new(), true) {
                        return Ok(outcome);
                    }
                }
                None => untested.push(end),
            }
        }
        let k = self.slots.len();
        let mut number = 0;
        loop {
            if self.stopped() {
                return Ok(Outcome::Cancelled);
            }
            let take = untested.len().min(k);
            let mut jobs: Vec<Job> = untested.drain(..take).collect();
            let busy: Vec<String> = jobs.iter().map(|j| j.tree.clone()).collect();
            jobs.extend(self.picks(k - jobs.len(), &busy));
            if jobs.is_empty() {
                return self.conclusion();
            }
            number += 1;
            self.out.send(Event::Round {
                number,
                rounds_left: self.search.rounds_left(k).saturating_sub(1),
                testing: jobs.iter().map(|j| j.hash.clone()).collect(),
            });
            if let Some(outcome) = self.round(jobs)? {
                return Ok(outcome);
            }
        }
    }

    /// Up to `n` commits to test, none with a tree of `busy` (the ends tested beside them) or
    /// one tested before: those answers are taken in at once, and the commits picked again.
    fn picks(&mut self, n: usize, busy: &[String]) -> Vec<Job> {
        let busy: Vec<&str> = busy.iter().map(String::as_str).collect();
        loop {
            let picks = self.search.picks(n, &busy);
            let mut known = false;
            let mut jobs = Vec::new();
            for i in picks {
                let (hash, tree) = (
                    self.search.dag().hash(i).to_owned(),
                    self.search.dag().tree(i).to_owned(),
                );
                let job = Job {
                    hash,
                    tree,
                    end: None,
                };
                match self.cached(&job.tree) {
                    Some(verdict) => {
                        known = true;
                        self.take(&job, verdict, String::new(), true);
                    }
                    None => jobs.push(job),
                }
            }
            if !known {
                return jobs;
            }
        }
    }

    fn cached(&self, tree: &str) -> Option<Verdict> {
        self.cache.get(&self.machine, &self.setup.command, tree)
    }

    /// Runs `jobs` side by side, one per worktree, taking their answers in as they come. A
    /// test whose answer is no longer needed (its commit left the candidates) is stopped.
    fn round(&mut self, jobs: Vec<Job>) -> Result<Option<Outcome>, String> {
        let dropped: Vec<AtomicBool> = jobs.iter().map(|_| AtomicBool::new(false)).collect();
        let mut slots = std::mem::take(&mut self.slots);
        let (tx, rx) = channel::<(usize, Result<Ran, String>)>();
        let (ws, setup, stop) = (self.ws, self.setup, self.stop);
        let mut ended = None;
        std::thread::scope(|scope| {
            for (j, (job, slot)) in jobs.iter().zip(slots.iter_mut()).enumerate() {
                let tx = tx.clone();
                let dropped = &dropped[j];
                scope.spawn(move || {
                    let stop = || dropped.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed);
                    let _ = tx.send((j, slot.test(ws, &job.hash, setup, &stop)));
                });
            }
            drop(tx);
            let mut running: HashSet<usize> = (0..jobs.len()).collect();
            while !running.is_empty() {
                let (j, result) = match rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(got) => got,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                running.remove(&j);
                if ended.is_some() || self.stopped() {
                    continue;
                }
                let job = &jobs[j];
                let answer = result.and_then(|ran| self.answer(job, ran));
                match answer {
                    Err(e) => ended = Some(Err(e)),
                    Ok(Some(outcome)) => ended = Some(Ok(outcome)),
                    Ok(None) => {}
                }
                if ended.is_some() {
                    dropped
                        .iter()
                        .for_each(|d| d.store(true, Ordering::Relaxed));
                    continue;
                }
                for &other in &running {
                    let moot = jobs[other].end.is_none()
                        && self
                            .search
                            .dag()
                            .index_of(&jobs[other].hash)
                            .is_some_and(|i| !self.search.is_candidate(i));
                    if moot && !dropped[other].swap(true, Ordering::Relaxed) {
                        self.out.send(Event::Dropped(jobs[other].hash.clone()));
                    }
                }
            }
        });
        self.slots = slots;
        if self.stopped() {
            return Ok(Some(Outcome::Cancelled));
        }
        ended.transpose()
    }

    /// What a test that came back says: `Some` when it ends the bisect. An exit code of 128 or
    /// more stops it on an end, as `git bisect run` would; on a commit in between, which a bisect
    /// one commit at a time might never have tested, it counts as skipped (not kept for its
    /// tree), with a warning.
    fn answer(&mut self, job: &Job, ran: Ran) -> Result<Option<Outcome>, String> {
        let verdict = match ran.ended {
            Ended::Exited(code) => match interpret(code) {
                Ok(verdict) => verdict,
                Err(e) if job.end.is_some() => {
                    let output = ran.output.trim();
                    return Err(format!("{} {e}\n{output}", short(&job.hash))
                        .trim_end()
                        .to_owned());
                }
                Err(_) => {
                    self.out.send(Event::Warning(format!(
                        "The test of {} ended with exit code {code} (a crash or a signal): it \
                         counts as skipped.",
                        short(&job.hash)
                    )));
                    return Ok(self.take(job, Verdict::Skip, ran.output, true));
                }
            },
            Ended::TimedOut => Verdict::TimedOut,
            Ended::Stopped => return Ok(None),
        };
        Ok(self.take(job, verdict, ran.output, false))
    }

    /// Takes in `verdict` for `job`, kept for its tree and told to the tab; `Some` when it
    /// ends the bisect (an end the test says the opposite of).
    fn take(
        &mut self,
        job: &Job,
        verdict: Verdict,
        output: String,
        cached: bool,
    ) -> Option<Outcome> {
        if !cached {
            self.cache
                .put(&self.machine, &self.setup.command, &job.tree, verdict);
        }
        let contradicts = job
            .end
            .is_some_and(|marked| !verdict.is_skip() && marked != verdict);
        let same_tree: Vec<String> = if contradicts {
            Vec::new()
        } else {
            self.search
                .mark_tree(&job.tree, verdict)
                .into_iter()
                .map(|i| self.search.dag().hash(i).to_owned())
                .collect()
        };
        self.out.send(Event::Tested {
            hash: job.hash.clone(),
            verdict,
            cached,
            output: output.clone(),
            same_tree,
        });
        if contradicts {
            let hash = job.hash.clone();
            return Some(match job.end {
                Some(Verdict::Good) => Outcome::GoodEndFails { hash, output },
                _ => Outcome::BadEndPasses { hash, output },
            });
        }
        self.send_remaining();
        None
    }

    fn send_remaining(&self) {
        let dag = self.search.dag();
        let left: HashSet<String> = self
            .search
            .candidates()
            .into_iter()
            .map(|i| dag.hash(i).to_owned())
            .collect();
        self.out.send(Event::Remaining(Arc::new(left)));
    }

    fn conclusion(&self) -> Result<Outcome, String> {
        let dag = self.search.dag();
        match self.search.conclusion() {
            Some(Conclusion::FirstBad(i)) => Ok(Outcome::FirstBad(dag.hash(i).to_owned())),
            Some(Conclusion::OneOf(all)) => Ok(Outcome::OneOf(
                all.into_iter().map(|i| dag.hash(i).to_owned()).collect(),
            )),
            None => Err("the bisect ran out of commits to test before it ended".to_owned()),
        }
    }
}
