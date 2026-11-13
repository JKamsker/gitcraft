//! Observed status changes and same-SHA history, separate from elapsed time and test reports.

use super::alignment::{self, Pair};
use super::workflow::Workflow;
use crate::ci::CheckState;
use crate::ci::timing::history::{History, RunKey};
use crate::ci::timing::{TimedJob, TimedRun};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    NewlyFailed,
    Fixed,
    ConsistentlyFailed,
    Passed,
    Unknown,
}

impl Transition {
    pub fn label(self) -> &'static str {
        match self {
            Self::NewlyFailed => "newly failed",
            Self::Fixed => "fixed",
            Self::ConsistentlyFailed => "consistently failed",
            Self::Passed => "passed in both",
            Self::Unknown => "unknown / unmatched / incomplete",
        }
    }
}

pub fn transition(now: Option<CheckState>, before: Option<CheckState>) -> Transition {
    match (now, before) {
        (Some(CheckState::Failed), Some(CheckState::Passed)) => Transition::NewlyFailed,
        (Some(CheckState::Passed), Some(CheckState::Failed)) => Transition::Fixed,
        (Some(CheckState::Failed), Some(CheckState::Failed)) => Transition::ConsistentlyFailed,
        (Some(CheckState::Passed), Some(CheckState::Passed)) => Transition::Passed,
        _ => Transition::Unknown,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub pair: Pair,
    pub transition: Transition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub pair: Pair,
    pub transition: Transition,
    pub steps: Vec<Step>,
}

pub fn compare(
    now: &TimedRun,
    before: &TimedRun,
    a: Option<&Workflow>,
    b: Option<&Workflow>,
) -> Vec<Job> {
    alignment::jobs(now, before, a, b)
        .into_iter()
        .map(|pair| {
            let now = pair.now.map(|i| &now.jobs[i]);
            let before = pair.before.map(|i| &before.jobs[i]);
            let steps = alignment::steps(
                now.map_or(&[], |job| job.steps.as_slice()),
                before.map_or(&[], |job| job.steps.as_slice()),
            )
            .into_iter()
            .map(|pair| Step {
                transition: transition(
                    pair.now.and_then(|i| now.map(|job| job.steps[i].state)),
                    pair.before
                        .and_then(|i| before.map(|job| job.steps[i].state)),
                ),
                pair,
            })
            .collect();
            Job {
                pair,
                transition: transition(now.map(|job| job.state), before.map(|job| job.state)),
                steps,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub key: RunKey,
    pub state: Option<CheckState>,
    pub url: String,
}

/// Only actual matching job/step outcomes count. Missing or ambiguous matches stay unknown.
pub fn observations(
    current: &TimedRun,
    index: usize,
    step: Option<usize>,
    history: &History,
) -> Vec<Observation> {
    history
        .runs
        .iter()
        .map(|run| {
            let job = same_sha_job(current, index, run);
            let target = current.jobs.get(index);
            let (state, url) = match (step, target, job) {
                (Some(index), Some(target), Some(job)) => step_observation(target, index, job),
                (None, _, Some(job)) => (Some(job.state), job.url.clone()),
                _ => (None, run.info.attempt_url()),
            };
            Observation {
                key: RunKey::of(&run.info),
                state,
                url,
            }
        })
        .collect()
}

/// Job `index` of `current` in `run`, only when `run` is the same workflow/branch at exactly the
/// same SHA and the job aligns unambiguously.
pub fn same_sha_job<'a>(
    current: &TimedRun,
    index: usize,
    run: &'a TimedRun,
) -> Option<&'a TimedJob> {
    if current.info.sha.is_empty()
        || current.info.sha != run.info.sha
        || !crate::ci::timing::history::same_workflow(&current.info, &run.info)
    {
        return None;
    }
    alignment::jobs(current, run, None, None)
        .iter()
        .find(|pair| pair.now == Some(index))
        .and_then(|pair| pair.before)
        .and_then(|i| run.jobs.get(i))
}

fn step_observation(
    target: &TimedJob,
    index: usize,
    job: &TimedJob,
) -> (Option<CheckState>, String) {
    let found = alignment::steps(&target.steps, &job.steps)
        .into_iter()
        .find(|pair| pair.now == Some(index))
        .and_then(|pair| pair.before)
        .and_then(|i| job.steps.get(i));
    found.map_or((None, job.url.clone()), |step| {
        (Some(step.state), job.step_url(step))
    })
}

pub fn intermittent(history: &[Observation]) -> bool {
    history
        .iter()
        .any(|seen| seen.state == Some(CheckState::Failed))
        && history
            .iter()
            .any(|seen| seen.state == Some(CheckState::Passed))
}

/// Job success alone provides no evidence that individual tests passed or caches hit.
pub const UNKNOWN_EVIDENCE: &str = "Individual test reports: unknown (not collected) until a job's logs are read. Cache hits/misses, toolchain versions and step log errors: unknown until then; exact recorded log links are available. Runner metadata is shown only when recorded. Timing differences are observations, not causes.";

#[cfg(test)]
mod tests;
