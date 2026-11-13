//! The logs of one picked job: in the run shown, in the run it is compared with, and in every
//! attempt read at the same SHA, each pinned to its run, attempt and job id. The jobs are
//! the ones the comparison and the same-SHA history already aligned; a job that doesn't match
//! unambiguously has no log here, said so, rather than another job's.

use std::collections::{BTreeSet, HashMap};

use super::Explorer;
use super::alignment;
use super::evidence::{self, LogEvidence, LogRead, Report};
use super::test_history::{self, Flaky, JobLog, TestComparison};
use crate::ci::CheckState;
use crate::ci::timing::history::{self, RunKey};
use crate::ci::timing::{TimedJob, TimedRun};

/// Which logs a report is of: it is shown only while the view shows exactly these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogTarget {
    pub run: RunKey,
    pub job: u64,
    pub baseline: Option<RunKey>,
}

pub fn target(explorer: &Explorer, index: usize) -> Option<LogTarget> {
    let job = explorer.run.jobs.get(index)?;
    Some(LogTarget {
        run: RunKey::of(&explorer.run.info),
        job: job.id,
        baseline: explorer
            .compared
            .as_ref()
            .map(|compared| RunKey::of(&compared.run.info)),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogReport {
    pub target: LogTarget,
    pub now: JobLog,
    pub before: Option<JobLog>,
    /// The picked job in every attempt read at this SHA, oldest first.
    pub history: Vec<JobLog>,
    pub tests: Option<TestComparison>,
    pub flaky: Vec<Flaky>,
}

const UNMATCHED: &str = "no job of this attempt matches the picked one unambiguously";

struct Reader<F> {
    fetch: F,
    read: HashMap<u64, LogRead>,
}

impl<F: FnMut(u64) -> Result<String, String>> Reader<F> {
    fn log(&mut self, run: &TimedRun, job: Option<&TimedJob>) -> JobLog {
        let key = RunKey::of(&run.info);
        let Some(job) = job.filter(|job| job.id > 0) else {
            return JobLog {
                key,
                job: 0,
                url: run.info.attempt_url(),
                read: LogRead::Unavailable(UNMATCHED.into()),
            };
        };
        let read = match self.read.get(&job.id) {
            Some(read) => read.clone(),
            None => {
                let finished = job.finished.is_some() && job.state != CheckState::Running;
                let read = from_text((self.fetch)(job.id), finished);
                self.read.insert(job.id, read.clone());
                read
            }
        };
        JobLog {
            key,
            job: job.id,
            url: job.url.clone(),
            read,
        }
    }
}

fn from_text(answer: Result<String, String>, finished: bool) -> LogRead {
    match answer {
        Ok(text) if text.trim().is_empty() => {
            LogRead::Unavailable("the forge returned an empty log".into())
        }
        Ok(text) => LogRead::Read(evidence::parse(&text, finished)),
        Err(error) => LogRead::Unavailable(
            error
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("it couldn't be read")
                .trim()
                .to_owned(),
        ),
    }
}

/// Reads the logs of job `index` of the run shown with `fetch` (a job id to its log text).
pub fn read(
    explorer: &Explorer,
    index: usize,
    fetch: impl FnMut(u64) -> Result<String, String>,
) -> Result<LogReport, String> {
    let target = target(explorer, index).ok_or("That job isn't in the run shown.")?;
    let mut reader = Reader {
        fetch,
        read: HashMap::new(),
    };
    let run = &explorer.run;
    let now = reader.log(run, run.jobs.get(index));
    let before = explorer.compared.as_ref().map(|compared| {
        let paired = explorer
            .diagnostics
            .iter()
            .find(|job| job.pair.now == Some(index))
            .and_then(|job| job.pair.before);
        reader.log(&compared.run, paired.and_then(|i| compared.run.jobs.get(i)))
    });
    let mut attempts: Vec<JobLog> = explorer
        .history
        .runs
        .iter()
        .map(|attempt| {
            if RunKey::of(&attempt.info) == target.run {
                return now.clone();
            }
            let same = !run.info.sha.is_empty()
                && attempt.info.sha == run.info.sha
                && history::same_workflow(&run.info, &attempt.info);
            let job = same
                .then(|| {
                    alignment::jobs(run, attempt, None, None)
                        .into_iter()
                        .find(|pair| pair.now == Some(index))
                        .and_then(|pair| pair.before)
                        .and_then(|i| attempt.jobs.get(i))
                })
                .flatten();
            reader.log(attempt, job)
        })
        .collect();
    if !attempts.iter().any(|log| log.key == target.run) {
        attempts.push(now.clone());
    }
    Ok(LogReport {
        target,
        tests: before
            .as_ref()
            .map(|before| test_history::compare(&now.read, &before.read)),
        flaky: test_history::flaky(&attempts),
        now,
        before,
        history: attempts,
    })
}

/// One thing the logs record, side by side. `None`: that side's log couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub what: String,
    pub before: Option<String>,
    pub now: Option<String>,
    /// Both logs were read and say different things.
    pub differs: bool,
}

const NOT_IN_LOG: &str = "not in the log";

fn report(e: &LogEvidence) -> String {
    let n = e.tests.len();
    match &e.report {
        Report::Complete => format!("{n} tests, complete"),
        Report::Partial(why) => format!("{n} tests, partial: {why}"),
        Report::Missing(why) => format!("unknown: {why}"),
    }
}

fn caches(e: &LogEvidence) -> String {
    if e.caches.is_empty() {
        return NOT_IN_LOG.into();
    }
    e.caches
        .iter()
        .map(|c| match c.key.as_str() {
            "" => c.kind.label().to_owned(),
            key => format!("{} {key}", c.kind.label()),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn value(list: &[(String, String)], key: &str) -> String {
    list.iter()
        .find(|(k, _)| k == key)
        .map_or_else(|| NOT_IN_LOG.to_owned(), |(_, v)| v.clone())
}

type Field = fn(&LogEvidence) -> &[(String, String)];

fn toolchain(e: &LogEvidence) -> &[(String, String)] {
    &e.toolchain
}

fn runner(e: &LogEvidence) -> &[(String, String)] {
    &e.runner
}

/// What the two logs record of tests, caches, toolchain, runner and errors.
pub fn rows(now: &LogRead, before: Option<&LogRead>) -> Vec<Row> {
    let (n, b) = (now.evidence(), before.and_then(LogRead::evidence));
    let mut list: Vec<(String, Box<dyn Fn(&LogEvidence) -> String>)> = vec![
        ("test report".into(), Box::new(report)),
        ("caches".into(), Box::new(caches)),
    ];
    let fields: [(&str, Field); 2] = [("toolchain", toolchain), ("runner", runner)];
    for (field, get) in fields {
        let keys: BTreeSet<String> = n
            .into_iter()
            .chain(b)
            .flat_map(|e| get(e).iter().map(|(k, _)| k.clone()))
            .collect();
        for key in keys {
            let shown = format!("{field}: {key}");
            list.push((shown, Box::new(move |e: &LogEvidence| value(get(e), &key))));
        }
    }
    list.push((
        "first error".into(),
        Box::new(|e| e.errors.first().cloned().unwrap_or_else(|| "none".into())),
    ));
    list.into_iter()
        .map(|(what, get)| {
            let (now, before) = (n.map(|e| get(e)), b.map(|e| get(e)));
            Row {
                differs: now.is_some() && before.is_some() && now != before,
                what,
                before,
                now,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
