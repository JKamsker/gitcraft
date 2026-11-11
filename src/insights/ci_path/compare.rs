//! A run against an earlier one of the same workflow: jobs aligned by name (a matrix job's
//! combination is in its name), steps by name within them, how much longer each took, which
//! became newly slow, whether the run lost parallelism, and what the forge recorded
//! differently of where they ran. Those differences are shown as recorded, never as causes.

use std::collections::HashMap;

use crate::ci::timing::{RunInfo, TimedJob, TimedRun, TimedStep};

/// Newly slow: at least so much longer…
pub const SLOWER_SHARE: f64 = 0.25;
/// …and at least so many seconds longer.
pub const SLOWER_SECONDS: i64 = 30;
/// The jobs together worked "about as long" within this share.
pub const SAME_WORK: f64 = 0.10;

/// Whether `now` seconds is newly slow against `before`.
pub fn newly_slow(now: i64, before: i64) -> bool {
    now - before >= SLOWER_SECONDS && now as f64 >= before as f64 * (1.0 + SLOWER_SHARE)
}

/// Seconds from start to end, when both are known.
fn took(started: Option<i64>, finished: Option<i64>) -> Option<i64> {
    Some((finished? - started?).max(0))
}

/// One step in both runs, or in one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepDelta {
    pub name: String,
    pub index: Option<usize>,
    pub now: Option<i64>,
    pub before: Option<i64>,
    pub slower: bool,
}

/// What the forge recorded differently of where a job ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub what: String,
    pub before: String,
    pub now: String,
}

/// One job in both runs, or in one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobDelta {
    pub name: String,
    /// Its index among the run's jobs, and among the earlier run's.
    pub now: Option<usize>,
    pub before: Option<usize>,
    /// Seconds it ran (queueing aside), when both ends are known.
    pub took_now: Option<i64>,
    pub took_before: Option<i64>,
    /// Seconds it waited for a runner.
    pub queued_now: Option<i64>,
    pub queued_before: Option<i64>,
    pub slower: bool,
    pub steps: Vec<StepDelta>,
    pub differences: Vec<Difference>,
}

impl JobDelta {
    /// Seconds longer (negative: shorter), when both are known.
    pub fn delta(&self) -> Option<i64> {
        Some(self.took_now? - self.took_before?)
    }
}

/// The run against the earlier one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub baseline: RunInfo,
    /// In the order of the run's jobs, then those only the earlier run had.
    pub jobs: Vec<JobDelta>,
    /// From the first job's start to the last one's end.
    pub span_now: Option<i64>,
    pub span_before: Option<i64>,
    /// What the jobs ran, added up.
    pub work_now: i64,
    pub work_before: i64,
    /// The run took markedly longer end to end while its jobs worked about as long: fewer of
    /// them ran at once.
    pub lost_parallelism: bool,
}

impl Comparison {
    pub fn slower(&self) -> impl Iterator<Item = &JobDelta> {
        self.jobs.iter().filter(|j| j.slower)
    }
}

/// The indices of `names`, each paired with its occurrence (`(name, 0)`, `(name, 1)`): the
/// same name twice is told apart by order.
fn keyed<'a>(names: impl Iterator<Item = &'a str>) -> Vec<(&'a str, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    names
        .map(|name| {
            let n = seen.entry(name).or_default();
            *n += 1;
            (name, *n - 1)
        })
        .collect()
}

/// Pairs the items of `now` with those of `before` with the same key: the pairs in `now`'s
/// order, then what only `before` has.
fn align<'a>(
    now: &[(&'a str, usize)],
    before: &[(&'a str, usize)],
) -> Vec<(Option<usize>, Option<usize>)> {
    let index: HashMap<(&str, usize), usize> =
        before.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    let mut taken = vec![false; before.len()];
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = now
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let b = index.get(key).copied();
            if let Some(b) = b {
                taken[b] = true;
            }
            (Some(i), b)
        })
        .collect();
    pairs.extend(
        taken
            .iter()
            .enumerate()
            .filter(|(_, t)| !**t)
            .map(|(b, _)| (None, Some(b))),
    );
    pairs
}

fn steps(now: &[TimedStep], before: &[TimedStep]) -> Vec<StepDelta> {
    super::alignment::steps(now, before)
        .into_iter()
        .map(|pair| {
            let (n, b) = (pair.now, pair.before);
            let n = n.map(|i| &now[i]);
            let b = b.map(|i| &before[i]);
            let took_now = n.and_then(|s| took(s.started, s.finished));
            let took_before = b.and_then(|s| took(s.started, s.finished));
            StepDelta {
                index: pair.now,
                name: n.or(b).map(|s| s.name.clone()).unwrap_or_default(),
                now: took_now,
                before: took_before,
                slower: took_now
                    .zip(took_before)
                    .is_some_and(|(n, b)| newly_slow(n, b)),
            }
        })
        .collect()
}

fn differences(now: &TimedJob, before: &TimedJob) -> Vec<Difference> {
    let mut keys: Vec<&str> = now.runner.iter().map(|(k, _)| k.as_str()).collect();
    for (k, _) in &before.runner {
        if !keys.contains(&k.as_str()) {
            keys.push(k);
        }
    }
    let get = |job: &TimedJob, key: &str| {
        job.runner
            .iter()
            .find(|(k, _)| k == key)
            .map_or_else(|| "not recorded".to_owned(), |(_, v)| v.clone())
    };
    keys.into_iter()
        .filter_map(|key| {
            let (a, b) = (get(now, key), get(before, key));
            (a != b).then(|| Difference {
                what: key.to_owned(),
                before: b,
                now: a,
            })
        })
        .collect()
}

/// From the first job's start to the last one's end, when any has both.
pub fn span(jobs: &[TimedJob]) -> Option<i64> {
    let start = jobs.iter().filter_map(|j| j.started).min()?;
    let end = jobs.iter().filter_map(|j| j.finished).max()?;
    Some((end - start).max(0))
}

/// What the jobs ran, added up (those with both ends known).
pub fn work(jobs: &[TimedJob]) -> i64 {
    jobs.iter()
        .filter_map(|j| took(j.started, j.finished))
        .sum()
}

/// `now` against the earlier run `before`.
pub fn compare(now: &TimedRun, before: &TimedRun) -> Comparison {
    let a = keyed(now.jobs.iter().map(|j| j.name.as_str()));
    let b = keyed(before.jobs.iter().map(|j| j.name.as_str()));
    let pairs = align(&a, &b)
        .into_iter()
        .map(|(now, before)| super::alignment::Pair {
            now,
            before,
            evidence: crate::insights::Evidence::Inferred,
        })
        .collect::<Vec<_>>();
    compare_aligned(now, before, &pairs)
}

pub fn compare_aligned(
    now: &TimedRun,
    before: &TimedRun,
    pairs: &[super::alignment::Pair],
) -> Comparison {
    let jobs = pairs
        .iter()
        .map(|pair| (pair.now, pair.before))
        .map(|(n, b)| {
            let job_now = n.map(|i| &now.jobs[i]);
            let job_before = b.map(|i| &before.jobs[i]);
            let took_now = job_now.and_then(|j| took(j.started, j.finished));
            let took_before = job_before.and_then(|j| took(j.started, j.finished));
            let queued = |j: &TimedJob| j.ready().zip(j.started).map(|(r, s)| (s - r).max(0));
            JobDelta {
                name: job_now
                    .or(job_before)
                    .map(|j| j.name.clone())
                    .unwrap_or_default(),
                now: n,
                before: b,
                took_now,
                took_before,
                queued_now: job_now.and_then(queued),
                queued_before: job_before.and_then(queued),
                slower: took_now
                    .zip(took_before)
                    .is_some_and(|(n, b)| newly_slow(n, b)),
                steps: match (job_now, job_before) {
                    (Some(x), Some(y)) => steps(&x.steps, &y.steps),
                    _ => Vec::new(),
                },
                differences: match (job_now, job_before) {
                    (Some(x), Some(y)) => differences(x, y),
                    _ => Vec::new(),
                },
            }
        })
        .collect();
    let (span_now, span_before) = (span(&now.jobs), span(&before.jobs));
    let (work_now, work_before) = (work(&now.jobs), work(&before.jobs));
    let same_work = work_before > 0
        && ((work_now - work_before).abs() as f64) <= work_before as f64 * SAME_WORK;
    let lost_parallelism = same_work
        && span_now
            .zip(span_before)
            .is_some_and(|(n, b)| newly_slow(n, b));
    Comparison {
        baseline: before.info.clone(),
        jobs,
        span_now,
        span_before,
        work_now,
        work_before,
        lost_parallelism,
    }
}
