//! Test-level signals from job logs, apart from job status and durations: how each test's
//! outcome changed between two runs, and which tests both failed and passed at one SHA (across
//! attempts and reruns, or retried within one). A test a log doesn't report is unknown, never
//! passed; a missing or partial report says so.

use std::collections::BTreeSet;

use super::diagnostics::{Transition, transition};
use super::evidence::{LogRead, Report, TestId};
use crate::ci::CheckState;
use crate::ci::timing::history::RunKey;

/// One job's log in one run attempt: the job it is, where to open it, and what it records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobLog {
    pub key: RunKey,
    /// The job's id; 0 when no job of the attempt matches the one picked.
    pub job: u64,
    pub url: String,
    pub read: LogRead,
}

/// Why `log`'s test outcomes can't be taken as all of them, if they can't.
pub fn gap(log: &LogRead) -> Option<String> {
    match log {
        LogRead::Unavailable(why) => Some(format!("log unavailable ({why})")),
        LogRead::Read(e) => match &e.report {
            Report::Complete => None,
            Report::Partial(why) => Some(format!("test report partial ({why})")),
            Report::Missing(why) => Some(format!("no test report ({why})")),
        },
    }
}

/// A test whose outcome isn't the same pass in both runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestChange {
    pub id: TestId,
    pub transition: Transition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestComparison {
    /// Newly failed first, then fixed, consistently failed, and unknown.
    pub changes: Vec<TestChange>,
    /// Tests that passed in both.
    pub passed: usize,
    /// Tests that were skipped in both.
    pub skipped: usize,
    /// What makes outcomes unknown on either side.
    pub gaps: Vec<String>,
}

fn rank(t: Transition) -> u8 {
    match t {
        Transition::NewlyFailed => 0,
        Transition::Fixed => 1,
        Transition::ConsistentlyFailed => 2,
        Transition::Unknown => 3,
        Transition::Passed => 4,
    }
}

fn outcome(log: &LogRead, id: &TestId) -> Option<CheckState> {
    log.evidence().and_then(|e| e.outcome(id))
}

/// Each test's outcome in `now` against `before`.
pub fn compare(now: &LogRead, before: &LogRead) -> TestComparison {
    let mut ids = BTreeSet::new();
    for log in [now, before] {
        if let Some(e) = log.evidence() {
            ids.extend(e.tests.keys().cloned());
        }
    }
    let mut out = TestComparison::default();
    for (side, log) in [("this run", now), ("the earlier run", before)] {
        if let Some(gap) = gap(log) {
            out.gaps.push(format!("{side}: {gap}"));
        }
    }
    for id in ids {
        let (a, b) = (outcome(now, &id), outcome(before, &id));
        let t = transition(a, b);
        if t == Transition::Passed {
            out.passed += 1;
        } else if a == Some(CheckState::Skipped) && b == a {
            out.skipped += 1;
        } else {
            out.changes.push(TestChange { id, transition: t });
        }
    }
    out.changes.sort_by_key(|c| rank(c.transition));
    out
}

/// One attempt's outcomes of a test: empty when its log can't say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    pub key: RunKey,
    pub outcomes: Vec<CheckState>,
    pub url: String,
}

/// A test with both a failure and a pass at one SHA: flake evidence, not a cause.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flaky {
    pub id: TestId,
    /// Every attempt read, oldest first.
    pub seen: Vec<Seen>,
}

fn word(state: Option<CheckState>) -> &'static str {
    match state {
        Some(CheckState::Passed) => "passed",
        Some(CheckState::Failed) => "failed",
        Some(CheckState::Skipped) => "skipped",
        Some(CheckState::Running) => "running",
        None => "unknown",
    }
}

impl Flaky {
    /// What happened in order: `failed ×2, then passed`.
    pub fn story(&self) -> String {
        let mut runs: Vec<(&str, usize)> = Vec::new();
        for seen in &self.seen {
            let words: Vec<&str> = if seen.outcomes.is_empty() {
                vec![word(None)]
            } else {
                seen.outcomes.iter().map(|s| word(Some(*s))).collect()
            };
            for w in words {
                match runs.last_mut() {
                    Some((last, n)) if *last == w => *n += 1,
                    _ => runs.push((w, 1)),
                }
            }
        }
        runs.iter()
            .map(|(w, n)| if *n > 1 { format!("{w} ×{n}") } else { (*w).to_owned() })
            .collect::<Vec<_>>()
            .join(", then ")
    }
}

/// The tests of `history` (one job's logs at one SHA, oldest first) that both failed and
/// passed.
pub fn flaky(history: &[JobLog]) -> Vec<Flaky> {
    let mut ids = BTreeSet::new();
    for log in history {
        if let Some(e) = log.read.evidence() {
            ids.extend(e.tests.keys().cloned());
        }
    }
    ids.into_iter()
        .filter_map(|id| {
            let seen: Vec<Seen> = history
                .iter()
                .map(|log| Seen {
                    key: log.key,
                    outcomes: log
                        .read
                        .evidence()
                        .and_then(|e| e.tests.get(&id))
                        .cloned()
                        .unwrap_or_default(),
                    url: log.url.clone(),
                })
                .collect();
            let all = || seen.iter().flat_map(|s| s.outcomes.iter());
            (all().any(|s| *s == CheckState::Failed) && all().any(|s| *s == CheckState::Passed))
                .then_some(Flaky { id, seen })
        })
        .collect()
}

#[cfg(test)]
mod tests;
