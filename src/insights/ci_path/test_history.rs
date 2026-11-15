//! Evidence from the exact logs of one job: the pinned run's job, its aligned job in the
//! baseline, and the same job in every recorded attempt at the same SHA. Per-test status
//! changes and same-SHA test history are kept apart from durations; a test a log doesn't
//! report stays unknown. Nothing here says why anything changed.

use std::collections::HashMap;
use std::sync::Arc;

mod assemble;

use super::Explorer;
use super::compare::Difference;
use super::diagnostics::{self, Observation, Transition};
use super::log_evidence::{self, LogEvidence, Outcome, Report, TestId};
use crate::ci::CheckState;
use crate::ci::timing::TimedJob;
use crate::ci::timing::history::RunKey;

/// So many tests with a signal are listed; the rest are counted.
pub(in crate::insights) const MAX_ROWS: usize = 200;
const MAX_EVIDENCE_TESTS: usize = 4 * log_evidence::MAX_TESTS;

/// One exact job log to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub key: RunKey,
    pub job: u64,
    pub url: String,
    /// The job had finished when the run was read; a running job's log is partial.
    pub finished: bool,
}

impl Target {
    fn of(key: RunKey, job: &TimedJob) -> Self {
        Self {
            key,
            job: job.id,
            url: job.url.clone(),
            finished: job.state != CheckState::Running,
        }
    }
}

/// Which logs belong to job `job` of the pinned run, bound to exact run attempts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub job: usize,
    pub now: Target,
    pub baseline: Option<RunKey>,
    /// The baseline's aligned job; `None` with a baseline: no unambiguous match.
    pub before: Option<Target>,
    /// Other attempts/runs at the same SHA whose job aligns.
    pub history: Vec<Target>,
    /// Same-SHA attempts without an aligned job: their outcomes are unknown.
    pub unaligned: Vec<RunKey>,
}

pub fn plan(explorer: &Explorer, job: usize) -> Option<Plan> {
    let current = RunKey::of(&explorer.run.info);
    let now = Target::of(current, explorer.run.jobs.get(job)?);
    let compared = explorer.compared.as_ref();
    let before = compared.and_then(|compared| {
        explorer
            .diagnostics
            .iter()
            .find(|entry| entry.pair.now == Some(job))
            .and_then(|entry| entry.pair.before)
            .and_then(|i| compared.run.jobs.get(i))
            .map(|found| Target::of(RunKey::of(&compared.run.info), found))
    });
    let (mut history, mut unaligned) = (Vec::new(), Vec::new());
    for run in explorer
        .history
        .runs
        .iter()
        .take(crate::ci::timing::history::LIMIT)
    {
        let key = RunKey::of(&run.info);
        if key == current {
            continue;
        }
        match diagnostics::same_sha_job(&explorer.run, job, run) {
            Some(found) => history.push(Target::of(key, found)),
            None => unaligned.push(key),
        }
    }
    Some(Plan {
        job,
        now,
        baseline: compared.map(|compared| RunKey::of(&compared.run.info)),
        before,
        history,
        unaligned,
    })
}

/// One log as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub target: Target,
    pub log: Result<Arc<LogEvidence>, String>,
}

impl Read {
    pub fn report(&self) -> Option<Report> {
        self.log.as_ref().ok().map(|log| log.report())
    }

    /// The test's outcome when every report of it in this log agrees.
    fn outcome(&self, test: &TestId) -> Option<Outcome> {
        self.log.as_ref().ok()?.tests.get(test)?.outcome()
    }

    /// Some report of the test in this log said it failed.
    fn failed(&self, test: &TestId) -> bool {
        self.log
            .as_ref()
            .ok()
            .and_then(|log| log.tests.get(test))
            .is_some_and(|seen| seen.failed())
    }
}

/// A test with a signal: a status change against the baseline, or varying same-SHA outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRow {
    pub test: TestId,
    pub transition: Transition,
    pub intermittent: bool,
    /// This run's log reported it more than once, failed at least once and otherwise not.
    pub ambiguous: bool,
    /// Its outcome in each read log at this SHA, oldest first.
    pub history: Vec<Observation>,
}

impl TestRow {
    pub fn label(&self) -> &'static str {
        if self.ambiguous {
            "ambiguous: failed in one report of it in this run's log, not in another"
        } else {
            self.transition.label()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobEvidence {
    pub plan: Plan,
    pub now: Read,
    pub before: Option<Read>,
    pub history: Vec<Read>,
    pub rows: Vec<TestRow>,
    /// Rows past [`MAX_ROWS`].
    pub more: usize,
    pub passed_both: usize,
    /// Tests without a definite outcome on both sides (absent, ignored, ambiguous, unread).
    pub unknown: usize,
    /// Cache, toolchain and report differences against the baseline's log, as recorded.
    pub differences: Vec<Difference>,
    /// Error annotations in this run's log that the baseline's log doesn't have.
    pub new_errors: Vec<String>,
}

/// Reads every log of `plan` with `fetch` (each exact job once) and compares them.
pub fn read(plan: Plan, fetch: impl FnMut(u64) -> Result<(String, bool), String>) -> JobEvidence {
    read_bounded(plan, fetch, MAX_EVIDENCE_TESTS)
}

fn read_bounded(
    plan: Plan,
    mut fetch: impl FnMut(u64) -> Result<(String, bool), String>,
    mut remaining: usize,
) -> JobEvidence {
    let mut cache: HashMap<u64, Result<Arc<LogEvidence>, String>> = HashMap::new();
    let mut read = |target: &Target| {
        let log = cache
            .entry(target.job)
            .or_insert_with(|| {
                if remaining == 0 {
                    return Ok(Arc::new(log_evidence::parse_bounded("", true, 0)));
                }
                fetch(target.job).map(|(text, cut)| {
                    let log =
                        log_evidence::parse_bounded(&text, cut || !target.finished, remaining);
                    remaining -= log.tests.len();
                    Arc::new(log)
                })
            })
            .clone();
        Read {
            target: target.clone(),
            log,
        }
    };
    let now = read(&plan.now);
    let before = plan.before.as_ref().map(&mut read);
    let history = plan.history.iter().map(&mut read).collect();
    assemble::assemble(plan, now, before, history)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod budget_tests;
