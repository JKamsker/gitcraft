//! Runs a test command at many commits at once, each worker in a scratch worktree of its own
//! on the repository's machine, and stores each verdict by tree as git-test does. Trees with a
//! result are not run again (unless asked), and two commits with the same tree run once.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::notes::{self, Verdict};
use super::scratch::Scratch;
use crate::git::{self, Stopper, run};

/// The most commits one run tests.
pub const MAX_COMMITS: usize = 1000;

/// The most workers one run has.
pub const MAX_WORKERS: usize = 16;

/// How many of the last lines a command printed are kept with its result.
pub const KEPT_LINES: usize = 200;

/// A commit to test, with its tree (what the result is kept by).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestJob {
    pub commit: String,
    pub tree: String,
}

/// How the test of one tree ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TestEnd {
    /// It ran and said so, with its exit code (`None`: ended by a signal).
    Ran { verdict: Verdict, code: Option<i32> },
    /// A result was stored already.
    Known(Verdict),
    /// It couldn't be run (the checkout failed…), or its result couldn't be stored.
    Failed(String),
}

impl TestEnd {
    /// The verdict, if there is one.
    pub fn verdict(&self) -> Option<Verdict> {
        match self {
            TestEnd::Ran { verdict, .. } | TestEnd::Known(verdict) => Some(*verdict),
            TestEnd::Failed(_) => None,
        }
    }
}

/// What a run tells as it goes, from its threads.
#[derive(Clone, Debug)]
pub enum TestEvent {
    /// The test of `tree` began.
    Started { tree: String },
    /// It ended, with the last lines it printed.
    Done {
        commit: String,
        tree: String,
        end: TestEnd,
        lines: Vec<String>,
    },
}

/// What to run, and how.
#[derive(Clone, Debug)]
pub struct TestPlan {
    /// The test's name (its notes ref and config key).
    pub name: String,
    /// What runs, as `sh` reads it, at the top of the worktree.
    pub command: String,
    /// How many commits at once.
    pub workers: usize,
    /// Run trees again that have a result already.
    pub retest: bool,
}

/// How a run went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub good: usize,
    pub bad: usize,
    /// Of those, how many had a result already.
    pub known: usize,
    /// Trees that couldn't be tested, with why.
    pub failed: Vec<(String, String)>,
    /// It was stopped before testing them all.
    pub stopped: bool,
    /// A scratch worktree couldn't be removed afterwards: why.
    pub cleanup: Option<String>,
}

/// Stops a run (every command it has going, and those it would start), from any thread.
#[derive(Clone, Default)]
pub struct RunStop(Arc<Mutex<StopState>>);

#[derive(Default)]
struct StopState {
    stopped: bool,
    going: Vec<Stopper>,
}

impl RunStop {
    pub fn stop(&self) {
        let going = match self.0.lock() {
            Ok(mut s) => {
                s.stopped = true;
                std::mem::take(&mut s.going)
            }
            Err(_) => Vec::new(),
        };
        for stopper in going {
            stopper.stop();
        }
    }

    pub fn stopped(&self) -> bool {
        self.0.lock().map(|s| s.stopped).unwrap_or(true)
    }

    /// A stopper for one command of the run: stopped already when the run is.
    pub fn command_stopper(&self) -> Stopper {
        let stopper = Stopper::default();
        match self.0.lock() {
            Ok(mut s) if !s.stopped => {
                s.going.retain(|st| !st.stopped());
                s.going.push(stopper.clone());
            }
            _ => stopper.stop(),
        }
        stopper
    }
}

/// The commits of `range` (`base..tip`, or a single commit) to test, oldest first as
/// git-test goes, each with its tree.
pub fn commits_to_test(repo: &Path, range: &str) -> Result<Vec<TestJob>, String> {
    let range = range.trim();
    if range.is_empty() {
        return Err("Say which commits to test".to_owned());
    }
    let limit = format!("--max-count={}", MAX_COMMITS + 1);
    let mut args = vec!["log", "--format=%H %T", &limit];
    if !range.contains("..") {
        args.push("--no-walk");
    }
    args.extend(["--end-of-options", range]);
    let out = run(Some(repo), &args)?;
    let mut jobs = parse_jobs(&out);
    if jobs.len() > MAX_COMMITS {
        return Err(format!(
            "{range} has more than {MAX_COMMITS} commits: test a shorter range"
        ));
    }
    jobs.reverse();
    Ok(jobs)
}

/// `%H %T` lines.
pub fn parse_jobs(out: &str) -> Vec<TestJob> {
    out.lines()
        .filter_map(|l| {
            let (commit, tree) = l.trim().split_once(' ')?;
            Some(TestJob {
                commit: commit.to_owned(),
                tree: tree.to_owned(),
            })
        })
        .collect()
}

/// The alias git runs the user's command with, at the top of the worktree, in its own `sh`
/// (so none is needed on PATH, on Windows either). What git sets for an alias run in a linked
/// worktree (`GIT_DIR`…) is unset: a test suite that makes repositories of its own would
/// otherwise have them all be this one.
pub fn command_alias(command: &str) -> String {
    format!(
        "alias.gitcraft-test=!unset GIT_DIR GIT_WORK_TREE GIT_PREFIX GIT_INDEX_FILE \
         GIT_CONFIG_PARAMETERS GIT_OPTIONAL_LOCKS; {command}"
    )
}

/// Runs `command` in the worktree `dir` until it ends or `stopper` stops it: its exit code
/// (`None`: a signal ended it) and its last [`KEPT_LINES`] lines, or why it couldn't run.
pub fn run_command(
    dir: &Path,
    command: &str,
    stopper: &Stopper,
) -> Result<(git::Ended, Vec<String>), String> {
    let alias = command_alias(command);
    let mut lines = VecDeque::new();
    let ended = git::stream(dir, &["-c", &alias, "gitcraft-test"], stopper, |line| {
        if lines.len() >= KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    })?;
    Ok((ended, lines.into()))
}

/// The verdict of a command that ended so: exit 0 is good, anything else bad (as git-test
/// reads it); `None` when it was stopped.
pub fn verdict_of(ended: &git::Ended) -> Option<Verdict> {
    if ended.stopped {
        return None;
    }
    Some(if ended.code == Some(0) {
        Verdict::Good
    } else {
        Verdict::Bad
    })
}

/// What is shared by the workers of a run.
struct Shared<'a> {
    repo: &'a Path,
    plan: &'a TestPlan,
    stop: &'a RunStop,
    queue: Mutex<VecDeque<TestJob>>,
    /// Notes are written one at a time: each moves the notes ref.
    writing: Mutex<()>,
    fallback_identity: bool,
    events: &'a (dyn Fn(TestEvent) + Sync),
    summary: Mutex<RunSummary>,
}

/// Tests `jobs` as `plan` says, `events` told as it goes, until done or `stop` stops it. For a
/// background thread: it takes as long as the tests. The scratch worktrees are removed in the
/// end, however it ends.
pub fn run_tests(
    repo: &Path,
    jobs: Vec<TestJob>,
    plan: &TestPlan,
    stop: &RunStop,
    events: &(dyn Fn(TestEvent) + Sync),
) -> Result<RunSummary, String> {
    if plan.command.trim().is_empty() {
        return Err("Type the command to test with".to_owned());
    }
    let known = if plan.retest {
        HashMap::new()
    } else {
        notes::read_results(repo, &plan.name)?
    };
    let mut summary = RunSummary::default();
    let mut queue = VecDeque::new();
    let mut seen = HashSet::new();
    for job in jobs {
        if !seen.insert(job.tree.clone()) {
            continue;
        }
        match known.get(&job.tree) {
            Some(&verdict) => {
                summary.known += 1;
                count(&mut summary, verdict);
                events(TestEvent::Done {
                    commit: job.commit,
                    tree: job.tree,
                    end: TestEnd::Known(verdict),
                    lines: Vec::new(),
                });
            }
            None => queue.push_back(job),
        }
    }
    let workers = plan.workers.clamp(1, MAX_WORKERS).min(queue.len());
    if workers == 0 {
        return Ok(summary);
    }
    // Made empty, at a commit that is there: each test checks its own out.
    let Some(start) = queue.iter().find(|j| is_commit(repo, &j.commit)) else {
        return Err("None of the commits to test is in the repository".to_owned());
    };
    let start = start.commit.clone();
    let mut scratches = Vec::new();
    for _ in 0..workers {
        if stop.stopped() {
            break;
        }
        scratches.push(Scratch::add_empty(repo, "test", &start)?);
    }
    let shared = Shared {
        repo,
        plan,
        stop,
        queue: Mutex::new(queue),
        writing: Mutex::new(()),
        fallback_identity: !notes::has_committer(repo),
        events,
        summary: Mutex::new(summary),
    };
    std::thread::scope(|s| {
        for scratch in &scratches {
            let shared = &shared;
            s.spawn(crate::connections::inherit(move || {
                work(shared, &scratch.path)
            }));
        }
    });
    let mut cleanup = Vec::new();
    for scratch in scratches {
        if let Err(e) = scratch.remove() {
            cleanup.push(e);
        }
    }
    let mut summary = shared
        .summary
        .into_inner()
        .map_err(|_| "a test worker failed".to_owned())?;
    summary.stopped = stop.stopped();
    summary.cleanup = cleanup.into_iter().next();
    Ok(summary)
}

/// Whether `rev` names a commit of `repo`.
fn is_commit(repo: &Path, rev: &str) -> bool {
    run(
        Some(repo),
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
    .is_ok()
}

fn count(summary: &mut RunSummary, verdict: Verdict) {
    match verdict {
        Verdict::Good => summary.good += 1,
        Verdict::Bad => summary.bad += 1,
    }
}

/// One worker: takes commits off the queue and tests each in its worktree `dir`.
fn work(shared: &Shared, dir: &Path) {
    loop {
        if shared.stop.stopped() {
            return;
        }
        let Some(job) = shared.queue.lock().ok().and_then(|mut q| q.pop_front()) else {
            return;
        };
        (shared.events)(TestEvent::Started {
            tree: job.tree.clone(),
        });
        // A test stopped part way says nothing, and is not counted.
        let Some((end, lines)) = test_one(shared, dir, &job) else {
            return;
        };
        if let Ok(mut summary) = shared.summary.lock() {
            match &end {
                TestEnd::Ran { verdict, .. } => count(&mut summary, *verdict),
                TestEnd::Known(_) => {}
                TestEnd::Failed(e) => summary.failed.push((job.tree.clone(), e.clone())),
            }
        }
        (shared.events)(TestEvent::Done {
            commit: job.commit,
            tree: job.tree,
            end,
            lines,
        });
    }
}

/// Checks `job` out in `dir`, runs the command there and stores what it said; `None` when the
/// run was stopped meanwhile.
fn test_one(shared: &Shared, dir: &Path, job: &TestJob) -> Option<(TestEnd, Vec<String>)> {
    let checkout = git::run(
        Some(dir),
        &["checkout", "--detach", "--force", "-q", &job.commit],
    );
    if let Err(e) = checkout {
        let said = format!("could not check out {}: {e}", short(&job.commit));
        return (!shared.stop.stopped()).then_some((TestEnd::Failed(said), Vec::new()));
    }
    let stopper = shared.stop.command_stopper();
    let (ended, lines) = match run_command(dir, &shared.plan.command, &stopper) {
        Ok(ran) => ran,
        Err(e) => return (!shared.stop.stopped()).then_some((TestEnd::Failed(e), Vec::new())),
    };
    let verdict = verdict_of(&ended)?;
    let written = {
        let _one_at_a_time = shared.writing.lock();
        notes::write_result(
            shared.repo,
            &shared.plan.name,
            &job.tree,
            verdict,
            shared.fallback_identity,
        )
    };
    Some(match written {
        Ok(()) => (
            TestEnd::Ran {
                verdict,
                code: ended.code,
            },
            lines,
        ),
        Err(e) => (TestEnd::Failed(e), lines),
    })
}

fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_are_read_from_hash_and_tree() {
        let out = "aaa 111\nbbb 222\n\n";
        assert_eq!(
            parse_jobs(out),
            [
                TestJob {
                    commit: "aaa".into(),
                    tree: "111".into()
                },
                TestJob {
                    commit: "bbb".into(),
                    tree: "222".into()
                }
            ]
        );
        assert!(parse_jobs("").is_empty());
    }

    #[test]
    fn exit_codes_read_as_git_test_reads_them() {
        let ended = |code, stopped| git::Ended { code, stopped };
        assert_eq!(verdict_of(&ended(Some(0), false)), Some(Verdict::Good));
        assert_eq!(verdict_of(&ended(Some(1), false)), Some(Verdict::Bad));
        assert_eq!(verdict_of(&ended(Some(125), false)), Some(Verdict::Bad));
        assert_eq!(verdict_of(&ended(None, false)), Some(Verdict::Bad));
        assert_eq!(verdict_of(&ended(Some(0), true)), None);
    }

    #[test]
    fn a_stopped_run_stops_the_commands_it_starts_after() {
        let stop = RunStop::default();
        let before = stop.command_stopper();
        assert!(!before.stopped());
        stop.stop();
        assert!(before.stopped());
        assert!(stop.command_stopper().stopped());
    }

    #[test]
    fn the_alias_unsets_what_git_sets_for_a_worktree() {
        let alias = command_alias("cargo test --all");
        assert!(alias.starts_with("alias.gitcraft-test=!unset GIT_DIR "));
        assert!(alias.ends_with("; cargo test --all"));
    }
}
