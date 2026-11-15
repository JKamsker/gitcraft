//! Streaming job-log state: binary boundaries, bounded identities and shard completeness.
use super::{binary::Binary, *};

pub(super) fn parse(text: &str, cut: bool, limit: usize) -> LogEvidence {
    let mut parser = Parser {
        evidence: LogEvidence {
            cut,
            ..Default::default()
        },
        limit,
        ..Default::default()
    };
    for raw in text.lines() {
        if raw.len() > MAX_PARSE_LINE {
            parser.evidence.cut = true;
            break;
        }
        parser.line(lines::plain(raw).trim());
        if parser.exhausted {
            break;
        }
    }
    parser.binary.publish(&parser.suite, &mut parser.evidence);
    parser.evidence.stopped |= parser.shard_open;
    parser.evidence
}

#[derive(Default)]
struct Parser {
    evidence: LogEvidence,
    suite: String,
    header: bool,
    binary: Binary,
    limit: usize,
    exhausted: bool,
    /// The shard runner exits on its first failing binary; only its end marker closes it.
    shard_open: bool,
}

impl Parser {
    fn line(&mut self, line: &str) {
        if line.starts_with("Running precompiled Windows test shard ") || lines::shard_binary(line)
        {
            self.shard_open = true;
        } else if lines::shard_passed(line) {
            self.shard_open = false;
            return;
        }
        if let Some(suite) = lines::binary(line) {
            if suite.len() > MAX_NAME_BYTES {
                self.exhaust();
                return;
            }
            self.binary.publish(&self.suite, &mut self.evidence);
            self.evidence.started += 1;
            (self.suite, self.header) = (suite, true);
        } else if let Some(count) = lines::announced(line) {
            if !std::mem::take(&mut self.header) {
                self.binary.publish(&self.suite, &mut self.evidence);
                self.evidence.started += 1;
                self.suite.clear();
            }
            self.binary.announced = Some(count);
        } else if line.starts_with("test result: ") {
            self.evidence.finished += 1;
            self.evidence.short |= !self.binary.reconcile(lines::summary(line));
            self.binary.publish(&self.suite, &mut self.evidence);
        } else if let Some((name, after)) = lines::test_start(line) {
            if !self.admit(name) {
                return;
            }
            match lines::outcome(after) {
                Some(outcome) => self.binary.record(name, outcome, false),
                None => {
                    self.binary.pending = Some(name.to_owned());
                }
            }
        } else if let (Some(outcome), Some(name)) =
            (lines::outcome(line), self.binary.pending.clone())
        {
            // Keep pending through bare output words, including a later contradictory FAILED.
            if self.admit(&name) {
                self.binary.record(&name, outcome, true);
            }
        } else if self.binary.failure_name(line) {
        } else if lines::stopped(line) {
            self.evidence.stopped = true;
        } else {
            notes(&mut self.evidence, line);
        }
    }

    fn admit(&mut self, name: &str) -> bool {
        if name.len() > MAX_NAME_BYTES
            || (!self.binary.tests.contains_key(name)
                && self.evidence.tests.len() + self.binary.tests.len() >= self.limit)
        {
            self.exhaust();
            false
        } else {
            true
        }
    }

    fn exhaust(&mut self) {
        self.evidence.cut = true;
        self.exhausted = true;
    }
}
