//! What one job's log text actually records: libtest outcomes and whether every test binary
//! that started also reported, cache hits and misses, toolchain versions and error
//! annotations. Only lines the log contains count; a missing or cut-short report leaves the
//! tests it would have listed unknown, never passed.

use std::collections::{BTreeMap, BTreeSet};

/// So many cache lines, toolchains and error annotations are kept per log.
pub const MAX_NOTES: usize = 16;
const MAX_LINE: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Ignored,
}

/// A test by the cargo target that ran it (`unittests src/lib.rs`) and its libtest path.
pub type TestId = (String, String);

/// How much of the test report the log holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// No libtest output recognized.
    Missing,
    /// A test binary started without its summary, reported fewer results than it announced,
    /// or the log was cut.
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
pub struct Cache {
    pub hit: bool,
    pub key: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogEvidence {
    /// `None`: this one log reported conflicting outcomes for the test.
    pub tests: BTreeMap<TestId, Option<Outcome>>,
    pub caches: Vec<Cache>,
    pub toolchains: BTreeSet<String>,
    pub errors: Vec<String>,
    started: usize,
    finished: usize,
    short: bool,
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
        } else if self.started == self.finished && !self.short && !self.cut {
            Report::Complete
        } else {
            Report::Incomplete
        }
    }
}

/// `text` is the log; `cut` says it was read only in part.
pub fn parse(text: &str, cut: bool) -> LogEvidence {
    let mut evidence = LogEvidence {
        cut,
        ..LogEvidence::default()
    };
    let mut suite = String::new();
    // Tests the open binary announced, and how many it has reported.
    let mut open: Option<(usize, usize)> = None;
    for raw in text.lines() {
        let line = plain(raw);
        let line = line.trim();
        if let Some(target) = cargo_target(line) {
            suite = target;
        } else if let Some(count) = announced(line) {
            evidence.started += 1;
            open = Some((count, 0));
        } else if line.starts_with("test result: ") {
            evidence.finished += 1;
            match open.take() {
                Some((announced, seen)) if seen >= announced => {}
                _ => evidence.short = true,
            }
        } else if let Some((name, outcome)) = test_line(line) {
            if let Some((_, seen)) = open.as_mut() {
                *seen += 1;
            }
            let entry = evidence
                .tests
                .entry((suite.clone(), name.to_owned()))
                .or_insert(Some(outcome));
            if *entry != Some(outcome) {
                *entry = None;
            }
        } else {
            notes(&mut evidence, line);
        }
    }
    evidence
}

/// GitHub's timestamp prefix, terminal colours and what a carriage return wrote over removed.
fn plain(raw: &str) -> String {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let raw = match raw.split_once(' ') {
        Some((stamp, rest)) if timestamp(stamp) => rest,
        _ => raw,
    };
    let raw = raw.trim_end_matches('\r');
    let raw = raw.rsplit('\r').next().unwrap_or(raw);
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.next_if_eq(&'[').is_some() {
                while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn timestamp(stamp: &str) -> bool {
    let b = stamp.as_bytes();
    b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[10] == b'T'
        && stamp.ends_with('Z')
}

/// cargo's `Running unittests src/lib.rs (target/debug/deps/gitcraft-0123abcd)` without the
/// build hash, or `Doc-tests gitcraft`.
fn cargo_target(line: &str) -> Option<String> {
    if let Some(name) = line.strip_prefix("Doc-tests ") {
        return Some(format!("doc-tests {}", name.trim()));
    }
    let target = line.strip_prefix("Running ")?;
    let (target, binary) = target.split_once(" (")?;
    binary
        .ends_with(')')
        .then(|| target.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// libtest's `running 12 tests` / `running 1 test`.
fn announced(line: &str) -> Option<usize> {
    let rest = line.strip_prefix("running ")?;
    let (count, word) = rest.split_once(' ')?;
    matches!(word, "test" | "tests")
        .then(|| count.parse().ok())
        .flatten()
}

/// libtest's `test path::name ... ok` (also `FAILED`, `ignored`, `ignored, reason`).
fn test_line(line: &str) -> Option<(&str, Outcome)> {
    let (name, result) = line.strip_prefix("test ")?.rsplit_once(" ... ")?;
    let outcome = match result.trim() {
        "ok" => Outcome::Passed,
        "FAILED" => Outcome::Failed,
        r if r == "ignored" || r.starts_with("ignored, ") => Outcome::Ignored,
        _ => return None,
    };
    (!name.is_empty()).then_some((name, outcome))
}

fn notes(evidence: &mut LogEvidence, line: &str) {
    let short = |text: &str| text.trim().chars().take(MAX_LINE).collect::<String>();
    let cache = if let Some(key) = line.strip_prefix("Cache restored from key:") {
        Some((true, short(key)))
    } else if let Some(key) = line.strip_prefix("Cache not found for input keys:") {
        Some((false, short(key)))
    } else if let Some(key) = line.strip_prefix("Restored from cache key ") {
        Some((true, short(key.trim_matches('"'))))
    } else if line == "No cache found." {
        Some((false, "(key not logged)".to_owned()))
    } else {
        None
    };
    if let Some((hit, key)) = cache {
        if evidence.caches.len() < MAX_NOTES {
            evidence.caches.push(Cache { hit, key });
        }
    } else if toolchain(line) {
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

/// A version line of the Rust toolchain: `rustc 1.95.0 (…)`, `cargo 1.95.0 (…)`.
fn toolchain(line: &str) -> bool {
    ["rustc ", "cargo ", "rustfmt ", "clippy "]
        .iter()
        .any(|tool| {
            line.strip_prefix(tool).is_some_and(|rest| {
                let version = rest.split(' ').next().unwrap_or_default();
                version.split('.').count() >= 3
                    && version
                        .split(['.', '-'])
                        .next()
                        .is_some_and(|major| major.parse::<u32>().is_ok())
            })
        })
}

#[cfg(test)]
mod tests;
