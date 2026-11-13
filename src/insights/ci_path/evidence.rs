//! What a job's log records beyond its status: each test's outcomes (libtest and nextest),
//! cache hits and misses, toolchain versions, the runner image, and error annotations. Only
//! what the log says counts: a job that hadn't finished, a test run without its summary, or
//! summaries that count other tests than were read make the test report partial, and a log
//! without test lines has no test report at all. Nothing here is inferred from timing.

use std::collections::{BTreeMap, BTreeSet};

use crate::ci::CheckState;

/// So many distinct error annotations are kept of a log.
pub const MAX_ERRORS: usize = 5;

/// A test by the run it was in (a test binary, or nextest's binary id) and its name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TestId {
    pub suite: String,
    pub name: String,
}

impl TestId {
    pub fn label(&self) -> String {
        if self.suite.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.suite)
        }
    }
}

/// How much of the job's tests the log reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// Every test run in the log has its summary, and the summaries count what was read.
    Complete,
    /// Some outcomes were read, but not provably all: why.
    Partial(String),
    /// No test outcome is in the log: why. Not a pass.
    Missing(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheKind {
    Hit,
    Miss,
    Saved,
}

impl CacheKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
            Self::Saved => "saved",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cache {
    pub kind: CacheKind,
    pub key: String,
}

/// What one job's log records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEvidence {
    /// Each test's outcomes in the order the log records them (several when it was retried).
    pub tests: BTreeMap<TestId, Vec<CheckState>>,
    pub report: Report,
    pub caches: Vec<Cache>,
    /// Tool and version, as printed (`rustc`, `cargo`, `go`, `set up`).
    pub toolchain: Vec<(String, String)>,
    /// What the log says of the machine: runner version, image, operating system.
    pub runner: Vec<(String, String)>,
    pub errors: Vec<String>,
}

impl LogEvidence {
    /// The test's last recorded outcome, if the log has it.
    pub fn outcome(&self, id: &TestId) -> Option<CheckState> {
        self.tests.get(id).and_then(|seen| seen.last().copied())
    }
}

/// A job's log as read: what it records, or why it couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogRead {
    Read(LogEvidence),
    Unavailable(String),
}

impl LogRead {
    pub fn evidence(&self) -> Option<&LogEvidence> {
        match self {
            Self::Read(evidence) => Some(evidence),
            Self::Unavailable(_) => None,
        }
    }
}

/// A line without GitHub's `2026-11-02T20:00:16.1234567Z ` prefix, BOM or carriage return.
fn content(line: &str) -> &str {
    let line = line.trim_end_matches('\r');
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    match line.split_once(' ') {
        Some((stamp, rest)) if is_stamp(stamp) => rest,
        _ if is_stamp(line) => "",
        _ => line,
    }
}

fn is_stamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[10] == b'T'
        && s.ends_with('Z')
}

/// `key: value` added once; another value of the same key is appended.
fn add(list: &mut Vec<(String, String)>, key: &str, value: &str) {
    let value = value.trim().trim_matches('\'').trim();
    if value.is_empty() {
        return;
    }
    match list.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) if v.split("; ").any(|seen| seen == value) => {}
        Some((_, v)) => {
            v.push_str("; ");
            v.push_str(value);
        }
        None => list.push((key.to_owned(), value.to_owned())),
    }
}

#[derive(Default)]
struct Parser {
    tests: BTreeMap<TestId, Vec<CheckState>>,
    caches: Vec<Cache>,
    toolchain: Vec<(String, String)>,
    runner: Vec<(String, String)>,
    errors: Vec<String>,
    suite: String,
    group: String,
    group_lines: Vec<String>,
    /// The tests of the run whose summary hasn't been read yet.
    section: BTreeSet<TestId>,
    /// A libtest line whose outcome came after the test's output.
    pending: Option<TestId>,
    counted: usize,
    read: usize,
    /// Nextest repeats failures after its summary: those aren't new outcomes.
    after_summary: bool,
}

impl Parser {
    fn record(&mut self, id: TestId, state: CheckState) {
        self.section.insert(id.clone());
        self.tests.entry(id).or_default().push(state);
    }

    fn close_section(&mut self, counted: usize) {
        self.counted += counted;
        self.read += self.section.len();
        self.section.clear();
        self.pending = None;
    }

    fn line(&mut self, text: &str) {
        if let Some(name) = text.strip_prefix("##[group]") {
            self.group = name.trim().to_owned();
            self.group_lines.clear();
            return;
        }
        if text.starts_with("##[endgroup]") {
            if self.group == "Operating System" {
                let os = self.group_lines.join(" ");
                add(&mut self.runner, "operating system", &os);
            }
            self.group.clear();
            return;
        }
        if let Some(error) = text.strip_prefix("##[error]") {
            let error = error.trim().to_owned();
            if self.errors.len() < MAX_ERRORS && !error.is_empty() && !self.errors.contains(&error)
            {
                self.errors.push(error);
            }
            return;
        }
        let t = text.trim();
        if !self.group.is_empty() && !t.is_empty() {
            self.group_lines.push(t.to_owned());
        }
        self.machine(t);
        self.cache(t);
        self.libtest(t);
        self.nextest(t);
    }

    fn machine(&mut self, t: &str) {
        if let Some(version) = t.strip_prefix("Current runner version:") {
            add(&mut self.runner, "runner version", version);
        }
        if self.group == "Runner Image" {
            if let Some(image) = t.strip_prefix("Image:") {
                add(&mut self.runner, "image", image);
            } else if let Some(version) = t.strip_prefix("Version:") {
                add(&mut self.runner, "image version", version);
            }
        }
        for tool in ["rustc", "cargo"] {
            if let Some(rest) = t.strip_prefix(tool).and_then(|r| r.strip_prefix(' '))
                && rest.starts_with(|c: char| c.is_ascii_digit())
            {
                add(&mut self.toolchain, tool, rest);
            }
        }
        if let Some(rest) = t.strip_prefix("go version ") {
            add(&mut self.toolchain, "go", rest);
        }
        if let Some(rest) = t.strip_prefix("Successfully set up ") {
            add(&mut self.toolchain, "set up", rest);
        }
    }

    fn cache(&mut self, t: &str) {
        let found = [
            ("Cache restored from key: ", CacheKind::Hit),
            ("Cache not found for input keys: ", CacheKind::Miss),
            ("Cache saved with key: ", CacheKind::Saved),
            ("Restored from cache key ", CacheKind::Hit),
        ]
        .into_iter()
        .find_map(|(marker, kind)| {
            t.split_once(marker).map(|(_, key)| {
                let key = key.split(" full match").next().unwrap_or(key);
                (kind, key.trim().trim_matches('"').to_owned())
            })
        })
        .or_else(|| (t == "No cache found.").then(|| (CacheKind::Miss, String::new())));
        if let Some((kind, key)) = found {
            let cache = Cache { kind, key };
            if self.caches.last() != Some(&cache) {
                self.caches.push(cache);
            }
        }
    }

    fn libtest(&mut self, t: &str) {
        if let Some(rest) = t.strip_prefix("Running ") {
            let suite = match rest.rsplit_once(" (") {
                Some((suite, _)) if rest.ends_with(')') => suite,
                _ => rest,
            };
            self.suite = suite.trim().to_owned();
        } else if let Some(rest) = t.strip_prefix("Doc-tests ") {
            self.suite = format!("doc-tests {}", rest.trim());
        } else if let Some(rest) = t.strip_prefix("test result: ") {
            let counted = rest
                .split(['.', ';'])
                .filter_map(|part| part.trim().split_once(' '))
                .filter(|(_, kind)| matches!(*kind, "passed" | "failed" | "ignored"))
                .filter_map(|(n, _)| n.parse::<usize>().ok())
                .sum();
            self.close_section(counted);
        } else if let Some((name, outcome)) = t
            .strip_prefix("test ")
            .and_then(|rest| rest.split_once(" ..."))
        {
            let name = name.trim().trim_end_matches(" - should panic").to_owned();
            let id = TestId {
                suite: self.suite.clone(),
                name,
            };
            match libtest_state(outcome.trim()) {
                Some(state) => self.record(id, state),
                None if outcome.trim().is_empty() => self.pending = Some(id),
                None => {}
            }
        } else if let Some(state) = libtest_state(t)
            && let Some(id) = self.pending.take()
        {
            self.record(id, state);
        }
    }

    fn nextest(&mut self, t: &str) {
        if t.starts_with("Starting ") && t.contains(" test") {
            self.after_summary = false;
            return;
        }
        if let Some(rest) = t.strip_prefix("Summary [") {
            let run = rest
                .split_once("] ")
                .and_then(|(_, r)| r.split(' ').next())
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            self.close_section(run);
            self.after_summary = true;
            return;
        }
        let t = match t.strip_prefix("TRY ") {
            Some(rest) => rest.split_once(' ').map_or(rest, |(_, r)| r),
            None => t,
        };
        let Some((status, rest)) = t.split_once(" [") else {
            return;
        };
        let state = match status {
            "PASS" | "LEAK" => CheckState::Passed,
            "FAIL" | "TIMEOUT" | "ABORT" | "LEAK-FAIL" => CheckState::Failed,
            s if s.starts_with("SIG") && s.len() > 3 => CheckState::Failed,
            _ => return,
        };
        let Some((_, test)) = rest.split_once("] ") else {
            return;
        };
        let Some((suite, name)) = test.trim().split_once(' ') else {
            return;
        };
        if !self.after_summary {
            let id = TestId {
                suite: suite.to_owned(),
                name: name.trim().to_owned(),
            };
            self.record(id, state);
        }
    }

    fn finish(self, finished: bool) -> LogEvidence {
        let open = !self.section.is_empty() || self.pending.is_some();
        let report = if self.tests.is_empty() {
            Report::Missing("no test results in the log".into())
        } else if !finished {
            Report::Partial("the job hadn't finished when its log was read".into())
        } else if open {
            Report::Partial("a test run has no summary: the log may be cut short".into())
        } else if self.counted != self.read {
            Report::Partial(format!(
                "the summaries count {} tests, {} were read",
                self.counted, self.read
            ))
        } else {
            Report::Complete
        };
        LogEvidence {
            tests: self.tests,
            report,
            caches: self.caches,
            toolchain: self.toolchain,
            runner: self.runner,
            errors: self.errors,
        }
    }
}

fn libtest_state(outcome: &str) -> Option<CheckState> {
    match outcome {
        "ok" => Some(CheckState::Passed),
        "FAILED" => Some(CheckState::Failed),
        o if o.starts_with("ignored") => Some(CheckState::Skipped),
        _ => None,
    }
}

/// What `text`, a job's whole log, records. `finished`: the job had ended when it was read.
pub fn parse(text: &str, finished: bool) -> LogEvidence {
    let mut parser = Parser::default();
    for line in text.lines() {
        parser.line(content(line));
    }
    parser.finish(finished)
}

#[cfg(test)]
pub(crate) mod tests;
