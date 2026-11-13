//! Evidence from the exact logs of one job: the pinned run's job, its aligned job in the
//! baseline, and the same job in every recorded attempt at the same SHA. Per-test status
//! changes and same-SHA test history are kept apart from durations; a test a log doesn't
//! report stays unknown. Nothing here says why anything changed.

use std::collections::{BTreeSet, HashMap};

use super::Explorer;
use super::compare::Difference;
use super::diagnostics::{self, Observation, Transition};
use super::log_evidence::{self, LogEvidence, Outcome, Report, TestId};
use crate::ci::CheckState;
use crate::ci::timing::TimedJob;
use crate::ci::timing::history::RunKey;

/// So many tests with a signal are listed; the rest are counted.
pub const MAX_ROWS: usize = 200;

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
    for run in &explorer.history.runs {
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
    pub log: Result<LogEvidence, String>,
}

impl Read {
    pub fn report(&self) -> Option<Report> {
        self.log.as_ref().ok().map(LogEvidence::report)
    }

    fn outcome(&self, test: &TestId) -> Option<Outcome> {
        self.log.as_ref().ok()?.tests.get(test).copied().flatten()
    }
}

/// A test with a signal: a status change against the baseline, or varying same-SHA outcomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRow {
    pub test: TestId,
    pub transition: Transition,
    pub intermittent: bool,
    /// Its outcome in each read log at this SHA, oldest first.
    pub history: Vec<Observation>,
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
pub fn read(
    plan: Plan,
    mut fetch: impl FnMut(u64) -> Result<(String, bool), String>,
) -> JobEvidence {
    let mut cache: HashMap<u64, Result<LogEvidence, String>> = HashMap::new();
    let mut read = |target: &Target| {
        let log = cache
            .entry(target.job)
            .or_insert_with(|| {
                fetch(target.job)
                    .map(|(text, cut)| log_evidence::parse(&text, cut || !target.finished))
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
    assemble(plan, now, before, history)
}

fn state(outcome: Option<Outcome>) -> Option<CheckState> {
    match outcome? {
        Outcome::Passed => Some(CheckState::Passed),
        Outcome::Failed => Some(CheckState::Failed),
        Outcome::Ignored => Some(CheckState::Skipped),
    }
}

pub(crate) fn assemble(
    plan: Plan,
    now: Read,
    before: Option<Read>,
    history: Vec<Read>,
) -> JobEvidence {
    let mut timeline: Vec<&Read> = history.iter().chain([&now]).collect();
    timeline.sort_by_key(|read| (read.target.key.run, read.target.key.attempt));
    let tests: BTreeSet<&TestId> = timeline
        .iter()
        .copied()
        .chain(before.as_ref())
        .filter_map(|read| read.log.as_ref().ok())
        .flat_map(|log| log.tests.keys())
        .collect();
    let (mut rows, mut passed_both, mut unknown) = (Vec::new(), 0, 0);
    for test in tests {
        let transition = diagnostics::transition(
            state(now.outcome(test)),
            before.as_ref().and_then(|read| state(read.outcome(test))),
        );
        let observed: Vec<Observation> = timeline
            .iter()
            .map(|read| Observation {
                key: read.target.key,
                state: state(read.outcome(test)),
                url: read.target.url.clone(),
            })
            .collect();
        let intermittent = diagnostics::intermittent(&observed);
        let failed_now = now.outcome(test) == Some(Outcome::Failed);
        let signal = intermittent
            || failed_now
            || matches!(transition, Transition::NewlyFailed | Transition::Fixed);
        match transition {
            _ if signal => rows.push(TestRow {
                test: test.clone(),
                transition,
                intermittent,
                history: observed,
            }),
            Transition::Passed => passed_both += 1,
            _ => unknown += 1,
        }
    }
    rows.sort_by_key(|row| (rank(row), row.test.clone()));
    let more = rows.len().saturating_sub(MAX_ROWS);
    rows.truncate(MAX_ROWS);
    let (differences, new_errors) = match before.as_ref().map(|read| &read.log) {
        Some(Ok(earlier)) => now
            .log
            .as_ref()
            .map(|log| (differences(log, earlier), new_errors(log, earlier)))
            .unwrap_or_default(),
        _ => Default::default(),
    };
    JobEvidence {
        plan,
        now,
        before,
        history,
        rows,
        more,
        passed_both,
        unknown,
        differences,
        new_errors,
    }
}

fn rank(row: &TestRow) -> u8 {
    match (row.transition, row.intermittent) {
        (Transition::NewlyFailed, _) => 0,
        (_, true) => 1,
        (Transition::ConsistentlyFailed, _) => 2,
        (Transition::Fixed, _) => 3,
        _ => 4,
    }
}

/// The log's cache lines, or that it records none.
pub fn caches(log: &LogEvidence) -> String {
    listed(
        log.caches
            .iter()
            .map(|c| format!("{} {}", if c.hit { "hit" } else { "miss" }, c.key))
            .collect(),
    )
}

/// The log's toolchain version lines, or that it records none.
pub fn toolchains(log: &LogEvidence) -> String {
    listed(log.toolchains.iter().cloned().collect())
}

fn differences(now: &LogEvidence, before: &LogEvidence) -> Vec<Difference> {
    let report = |log: &LogEvidence| log.report().label().to_owned();
    [
        ("cache", caches(now), caches(before)),
        ("toolchain", toolchains(now), toolchains(before)),
        ("test report", report(now), report(before)),
    ]
    .into_iter()
    .filter(|(_, now, before)| now != before)
    .map(|(what, now, before)| Difference {
        what: what.to_owned(),
        before,
        now,
    })
    .collect()
}

fn listed(items: Vec<String>) -> String {
    if items.is_empty() {
        "not recorded in the log".to_owned()
    } else {
        items.join("; ")
    }
}

fn new_errors(now: &LogEvidence, before: &LogEvidence) -> Vec<String> {
    now.errors
        .iter()
        .filter(|error| !before.errors.contains(error))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
