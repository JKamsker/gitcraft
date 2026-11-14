//! What one job's log text actually records: libtest outcomes and whether every test binary
//! that started also reported, cache hits and misses, toolchain versions and error
//! annotations. Only lines the log contains count; a missing or cut-short report leaves the
//! tests it would have listed unknown, never passed.

use std::collections::{BTreeMap, BTreeSet};

mod binary;
mod lines;
mod parser;

/// Limits both retained test identities and their maximum byte lengths.
pub(in crate::insights) const MAX_TESTS: usize = 8192;
const MAX_NAME_BYTES: usize = 1024;
const MAX_PARSE_LINE: usize = 16 * 1024;

/// So many cache lines, toolchains and error annotations are kept per log.
pub(in crate::insights) const MAX_NOTES: usize = 16;
const MAX_LINE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::insights) enum Outcome {
    Passed,
    Failed,
    Ignored,
}

/// A test by the suite that ran it (the test binary without its build hash, and for cargo the
/// target: `gitcraft (unittests src/lib.rs)`) and its libtest path.
pub(in crate::insights) type TestId = (String, String);

/// Every outcome one log reported for one test: a test binary run twice, or two binaries the
/// log can't tell apart (same name, same test path), can report it more than once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::insights) struct Seen {
    passed: bool,
    failed: bool,
    ignored: bool,
    uncertain: bool,
}

impl Seen {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Passed => self.passed = true,
            Outcome::Failed => self.failed = true,
            Outcome::Ignored => self.ignored = true,
        }
    }

    fn merge(&mut self, other: Self) {
        self.passed |= other.passed;
        self.failed |= other.failed;
        self.ignored |= other.ignored;
        self.uncertain |= other.uncertain;
    }

    /// The outcome every report agrees on; `None` when they conflict.
    pub(in crate::insights) fn outcome(self) -> Option<Outcome> {
        if self.uncertain {
            return None;
        }
        match (self.passed, self.failed, self.ignored) {
            (true, false, false) => Some(Outcome::Passed),
            (false, true, false) => Some(Outcome::Failed),
            (false, false, true) => Some(Outcome::Ignored),
            _ => None,
        }
    }

    /// At least one report said it failed.
    pub(in crate::insights) fn failed(self) -> bool {
        self.failed
    }
}

/// How much of the test report the log holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// No libtest output recognized.
    Missing,
    /// A test binary started without its summary, reported fewer results than it announced,
    /// cargo stopped at a failing binary, or the log was cut.
    Incomplete,
    Complete,
}

impl Report {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "no test report recognized in the log (test outcomes unknown)",
            Self::Incomplete => {
                "test report incomplete (tests absent from it are unknown, not passed)"
            }
            Self::Complete => "every test binary that started reported a summary",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::insights) struct Cache {
    pub(in crate::insights) hit: bool,
    pub(in crate::insights) key: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogEvidence {
    pub(in crate::insights) tests: BTreeMap<TestId, Seen>,
    pub(in crate::insights) caches: Vec<Cache>,
    pub(in crate::insights) toolchains: BTreeSet<String>,
    pub(in crate::insights) errors: Vec<String>,
    /// Test binaries started: a cargo or shard-script header, or `running N tests` without one.
    started: usize,
    finished: usize,
    short: bool,
    /// cargo stopped at a failing binary; later binaries never ran.
    stopped: bool,
    cut: bool,
}

impl LogEvidence {
    pub fn report(&self) -> Report {
        if self.cut {
            return Report::Incomplete;
        }
        if self.started == 0 && self.finished == 0 {
            if self.tests.is_empty() {
                Report::Missing
            } else {
                Report::Incomplete
            }
        } else if self.started == self.finished && !self.short && !self.stopped && !self.cut {
            Report::Complete
        } else {
            Report::Incomplete
        }
    }

    /// The log's cache lines, or that it records none.
    pub fn caches(&self) -> String {
        listed(
            self.caches
                .iter()
                .map(|c| format!("{} {}", if c.hit { "hit" } else { "miss" }, c.key))
                .collect(),
        )
    }

    /// The log's toolchain version lines, or that it records none.
    pub fn toolchains(&self) -> String {
        listed(self.toolchains.iter().cloned().collect())
    }
}

fn listed(items: Vec<String>) -> String {
    if items.is_empty() {
        "not recorded in the log".to_owned()
    } else {
        items.join("; ")
    }
}

/// `text` is the log; `cut` says it was read only in part.
pub(in crate::insights) fn parse(text: &str, cut: bool) -> LogEvidence {
    parse_bounded(text, cut, MAX_TESTS)
}

/// A caller may allocate a smaller share of its aggregate evidence budget to this log.
pub(in crate::insights) fn parse_bounded(text: &str, cut: bool, limit: usize) -> LogEvidence {
    parser::parse(text, cut, limit.min(MAX_TESTS))
}

fn notes(evidence: &mut LogEvidence, line: &str) {
    let short = |text: &str| text.trim().chars().take(MAX_LINE).collect::<String>();
    if let Some((hit, key)) = lines::cache(line, short) {
        if evidence.caches.len() < MAX_NOTES {
            evidence.caches.push(Cache { hit, key });
        }
    } else if lines::toolchain(line) {
        if evidence.toolchains.len() < MAX_NOTES {
            evidence.toolchains.insert(short(line));
        }
    } else if let Some((_, message)) = line.split_once("##[error]") {
        let message = short(message);
        if evidence.errors.len() < MAX_NOTES && !evidence.errors.contains(&message) {
            evidence.errors.push(message);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod review_tests;

#[cfg(test)]
mod reconciliation_tests;
