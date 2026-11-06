//! What a search for the interaction says.

use crate::git::{Found, Interaction, Probe, SearchProgress, Side};

/// The finding, in a sentence, for the branches `a` and `b`.
pub(crate) fn found_line(found: &Found, a: &str, b: &str, budget: usize) -> String {
    match found {
        Found::Together {
            commits,
            minimal,
            gaps: true,
        } if *minimal => match commits.len() {
            2 => "These two commits fail together; leaving either out passes or can't be \
                  tested (it doesn't apply without the other):"
                .to_owned(),
            n => format!(
                "These {n} commits fail together; leaving any one out passes or can't be tested \
                 (it doesn't apply without the others):"
            ),
        },
        Found::Together {
            commits, minimal, ..
        } => {
            let sides = (
                commits.iter().any(|c| c.side == Side::A),
                commits.iter().any(|c| c.side == Side::B),
            );
            match (commits.len(), minimal, sides) {
                (_, false, _) => format!(
                    "Out of tests ({budget}): these {} commits fail together; fewer of them may \
                     too",
                    commits.len()
                ),
                (1, true, _) => {
                    "This commit fails on its own, applied where the two diverged:".to_owned()
                }
                (2, true, (true, true)) => {
                    "These two commits fail together; either alone passes:".to_owned()
                }
                (n, true, _) => {
                    format!("These {n} commits fail together; leaving out any one of them passes:")
                }
            }
        }
        Found::PassTogether => format!("{a} and {b} pass together: there is nothing to find."),
        Found::FailsAlone(Side::A) => {
            format!("{a} fails on its own: it isn't the two together. Test its commits instead.")
        }
        Found::FailsAlone(Side::B) => {
            format!("{b} fails on its own: it isn't the two together. Test its commits instead.")
        }
        Found::DontApply => format!(
            "The commits of {a} and {b} don't apply together: rehearse the merge to see the \
             conflicts."
        ),
        Found::Flaky => {
            "The two together don't give the same result twice: the test is flaky.".to_owned()
        }
    }
}

/// The search's numbers: `12 tests run · 1 flaky result set aside`.
pub(crate) fn tally_line(i: &Interaction) -> String {
    let mut line = match i.tests {
        1 => "1 test run".to_owned(),
        n => format!("{n} tests run"),
    };
    match i.flaky.len() {
        0 => {}
        1 => line.push_str(" · 1 result didn't repeat and was set aside"),
        n => line.push_str(&format!(" · {n} results didn't repeat and were set aside")),
    }
    line
}

/// Where a search is: `Test 7 of at most 40: 3 commits fail`.
pub(crate) fn progress_line(p: &SearchProgress) -> String {
    let said = match p.probe {
        Probe::Pass => "pass",
        Probe::Fail => "fail",
        Probe::Unresolved => "can't tell (they don't apply, or it didn't repeat)",
    };
    let commits = if p.size == 1 { "commit" } else { "commits" };
    format!(
        "Test {} of at most {}: {} {commits} {said}",
        p.tests, p.budget, p.size
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Change;

    fn change(side: Side) -> Change {
        Change {
            commit: "c".into(),
            summary: "s".into(),
            side,
        }
    }

    #[test]
    fn the_pair_reads_as_the_issue_asks() {
        let found = Found::Together {
            commits: vec![change(Side::A), change(Side::B)],
            minimal: true,
            gaps: false,
        };
        assert_eq!(
            found_line(&found, "a", "b", 40),
            "These two commits fail together; either alone passes:"
        );
        let three = Found::Together {
            commits: vec![change(Side::A), change(Side::A), change(Side::B)],
            minimal: true,
            gaps: false,
        };
        assert!(found_line(&three, "a", "b", 40).starts_with("These 3 commits fail together"));
        let cut = Found::Together {
            commits: vec![change(Side::A); 5],
            minimal: false,
            gaps: false,
        };
        assert!(found_line(&cut, "a", "b", 9).starts_with("Out of tests (9): these 5"));
        assert_eq!(
            found_line(&Found::PassTogether, "x", "ü y", 1),
            "x and ü y pass together: there is nothing to find."
        );
        assert!(found_line(&Found::FailsAlone(Side::B), "x", "y", 1).starts_with("y fails"));
        let gaps = Found::Together {
            commits: vec![change(Side::A), change(Side::B)],
            minimal: true,
            gaps: true,
        };
        assert!(found_line(&gaps, "a", "b", 40).contains("passes or can't be tested"));
    }

    #[test]
    fn progress_and_tally() {
        let p = SearchProgress {
            tests: 7,
            budget: 40,
            size: 3,
            probe: Probe::Fail,
        };
        assert_eq!(progress_line(&p), "Test 7 of at most 40: 3 commits fail");
        let i = Interaction {
            found: Found::Flaky,
            base: String::new(),
            changes: Vec::new(),
            tests: 3,
            flaky: vec![Vec::new()],
        };
        assert_eq!(
            tally_line(&i),
            "3 tests run · 1 result didn't repeat and was set aside"
        );
    }
}
