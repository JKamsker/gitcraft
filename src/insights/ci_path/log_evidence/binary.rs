//! One libtest binary's bounded outcomes, reconciled with its summary before publication.
use super::*;

#[derive(Default)]
pub(super) struct Binary {
    pub announced: Option<usize>,
    pub tests: BTreeMap<String, Seen>,
    pub pending: Option<String>,
    failures: bool,
}

impl Binary {
    pub fn failure_name(&mut self, line: &str) -> bool {
        if line == "failures:" {
            self.failures = true;
            return true;
        }
        if self.failures
            && let Some(seen) = self.tests.get_mut(line)
        {
            seen.add(Outcome::Failed);
            return true;
        }
        false
    }

    pub fn record(&mut self, name: &str, outcome: Outcome, provisional: bool) {
        let seen = self.tests.entry(name.to_owned()).or_default();
        seen.add(outcome);
        // A bare word may be the test's own stdout. Its summary must corroborate it.
        seen.uncertain |= provisional && outcome == Outcome::Passed;
        if !provisional && self.pending.as_deref() == Some(name) {
            self.pending = None;
        }
    }

    pub fn reconcile(&mut self, summary: Option<[usize; 3]>) -> bool {
        let mut observed = [0usize; 3];
        for seen in self.tests.values() {
            for (count, present) in
                observed
                    .iter_mut()
                    .zip([seen.passed, seen.failed, seen.ignored])
            {
                *count += usize::from(present);
            }
        }
        let complete = summary == Some(observed)
            && self.announced == Some(self.tests.len())
            && observed.iter().sum::<usize>() == self.tests.len();
        for seen in self.tests.values_mut() {
            // Inconsistent totals cannot identify which named outcome to trust. Even a
            // named pass stays unknown; recorded failures still get a visible row.
            seen.uncertain = !complete;
        }
        complete
    }

    pub fn publish(&mut self, suite: &str, evidence: &mut LogEvidence) {
        for (name, seen) in std::mem::take(&mut self.tests) {
            evidence
                .tests
                .entry((suite.to_owned(), name))
                .or_default()
                .merge(seen);
        }
        (self.announced, self.pending, self.failures) = (None, None, false);
    }
}
