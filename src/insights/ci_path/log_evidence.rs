//! What one job's log text actually records: libtest outcomes and whether every test binary
//! that started also reported, cache hits and misses, toolchain versions and error
//! annotations. Only lines the log contains count; a missing or cut-short report leaves the
//! tests it would have listed unknown, never passed.

use std::collections::{BTreeMap, BTreeSet};

mod lines;

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
}

impl Seen {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Passed => self.passed = true,
            Outcome::Failed => self.failed = true,
            Outcome::Ignored => self.ignored = true,
        }
    }

    /// The outcome every report agrees on; `None` when they conflict.
    pub(in crate::insights) fn outcome(self) -> Option<Outcome> {
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
    let mut parser = Parser {
        evidence: LogEvidence {
            cut,
            ..LogEvidence::default()
        },
        ..Parser::default()
    };
    for raw in text.lines() {
        parser.line(lines::plain(raw).trim());
    }
    parser.evidence
}

/// Reads libtest's pretty output the way ci/libtest_watchdog.py does, `--nocapture` included.
#[derive(Default)]
struct Parser {
    evidence: LogEvidence,
    suite: String,
    /// A binary's header was seen and its `running N tests` not yet.
    header: bool,
    /// Tests the open binary announced, and how many it has reported.
    open: Option<(usize, usize)>,
    /// A test that started (`test name ... `) and whose outcome is on a later line.
    pending: Option<String>,
}

impl Parser {
    fn line(&mut self, line: &str) {
        let evidence = &mut self.evidence;
        if let Some(suite) = lines::binary(line) {
            evidence.started += 1;
            (self.suite, self.header, self.open, self.pending) = (suite, true, None, None);
        } else if let Some(count) = lines::announced(line) {
            if !std::mem::take(&mut self.header) {
                evidence.started += 1;
            }
            (self.open, self.pending) = (Some((count, 0)), None);
        } else if line.starts_with("test result: ") {
            evidence.finished += 1;
            self.pending = None;
            match self.open.take() {
                Some((announced, seen)) if seen >= announced => {}
                _ => evidence.short = true,
            }
        } else if let Some((name, after)) = lines::test_start(line) {
            match lines::outcome(after) {
                Some(outcome) => self.record(name.to_owned(), outcome),
                None => self.pending = Some(name.to_owned()),
            }
        } else if let Some(outcome) = lines::outcome(line).filter(|_| self.pending.is_some()) {
            let name = self.pending.take().unwrap_or_default();
            self.record(name, outcome);
        } else if lines::stopped(line) {
            evidence.stopped = true;
        } else {
            notes(evidence, line);
        }
    }

    fn record(&mut self, name: String, outcome: Outcome) {
        self.pending = None;
        if let Some((_, seen)) = self.open.as_mut() {
            *seen += 1;
        }
        let test = (self.suite.clone(), name);
        self.evidence.tests.entry(test).or_default().add(outcome);
    }
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
