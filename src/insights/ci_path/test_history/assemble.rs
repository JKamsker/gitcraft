//! Select the best bounded set of signal rows before allocating per-attempt observations.
use super::*;

fn state(outcome: Option<Outcome>) -> Option<CheckState> {
    match outcome? {
        Outcome::Passed => Some(CheckState::Passed),
        Outcome::Failed => Some(CheckState::Failed),
        Outcome::Ignored => Some(CheckState::Skipped),
    }
}

struct Candidate<'a> {
    test: &'a TestId,
    transition: Transition,
    intermittent: bool,
    ambiguous: bool,
}

impl Candidate<'_> {
    fn key(&self) -> (u8, &TestId) {
        let rank = match (self.transition, self.intermittent) {
            (Transition::NewlyFailed, _) => 0,
            (_, true) => 1,
            (Transition::ConsistentlyFailed, _) => 2,
            _ if self.ambiguous => 2,
            (Transition::Fixed, _) => 3,
            _ => 4,
        };
        (rank, self.test)
    }

    fn into_row(self, timeline: &[&Read]) -> TestRow {
        TestRow {
            test: self.test.clone(),
            transition: self.transition,
            intermittent: self.intermittent,
            ambiguous: self.ambiguous,
            history: timeline
                .iter()
                .map(|read| Observation {
                    key: read.target.key,
                    state: state(read.outcome(self.test)),
                    url: read.target.url.clone(),
                })
                .collect(),
        }
    }
}

pub(super) fn assemble(
    plan: Plan,
    now: Read,
    before: Option<Read>,
    history: Vec<Read>,
) -> JobEvidence {
    let mut timeline: Vec<&Read> = history.iter().chain([&now]).collect();
    timeline.sort_by_key(|read| (read.target.key.run, read.target.key.attempt));
    // A merge of sorted map keys needs only one cursor per log, rather than a union
    // allocation for every test. Log maps already obey the aggregate entry budget.
    let mut cursors: Vec<_> = timeline
        .iter()
        .copied()
        .chain(before.as_ref())
        .filter_map(|read| read.log.as_ref().ok())
        .map(|log| log.tests.keys().peekable())
        .collect();
    let (mut kept, mut signals, mut passed_both, mut unknown) =
        (Vec::<Candidate<'_>>::with_capacity(MAX_ROWS), 0usize, 0, 0);
    while let Some(test) = cursors
        .iter_mut()
        .filter_map(|cursor| cursor.peek().copied())
        .min()
    {
        for cursor in &mut cursors {
            if cursor.peek().copied() == Some(test) {
                cursor.next();
            }
        }
        let transition = diagnostics::transition(
            state(now.outcome(test)),
            before.as_ref().and_then(|read| state(read.outcome(test))),
        );
        let intermittent = timeline
            .iter()
            .any(|read| read.outcome(test) == Some(Outcome::Passed))
            && timeline
                .iter()
                .any(|read| read.outcome(test) == Some(Outcome::Failed));
        let failed_now = now.failed(test);
        let signal = intermittent
            || failed_now
            || matches!(transition, Transition::NewlyFailed | Transition::Fixed);
        if signal {
            signals += 1;
            let candidate = Candidate {
                test,
                transition,
                intermittent,
                ambiguous: failed_now && now.outcome(test).is_none(),
            };
            let at = kept.partition_point(|row| row.key() <= candidate.key());
            if at < MAX_ROWS {
                if kept.len() == MAX_ROWS {
                    kept.pop();
                }
                kept.insert(at, candidate);
            }
        } else if transition == Transition::Passed {
            passed_both += 1;
        } else {
            unknown += 1;
        }
    }
    let more = signals.saturating_sub(kept.len());
    let rows = kept
        .into_iter()
        .map(|candidate| candidate.into_row(&timeline))
        .collect();
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

fn differences(now: &LogEvidence, before: &LogEvidence) -> Vec<Difference> {
    let report = |log: &LogEvidence| log.report().label().to_owned();
    [
        ("cache", now.caches(), before.caches()),
        ("toolchain", now.toolchains(), before.toolchains()),
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

fn new_errors(now: &LogEvidence, before: &LogEvidence) -> Vec<String> {
    now.errors
        .iter()
        .filter(|error| !before.errors.contains(error))
        .cloned()
        .collect()
}
