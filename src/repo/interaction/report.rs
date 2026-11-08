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
            2 => crate::i18n::text("repo-core-2812").to_owned(),
            n => crate::i18n::message("repo-core-2813", &[("n", (n).into())]),
        },
        Found::Together {
            commits, minimal, ..
        } => {
            let sides = (
                commits.iter().any(|c| c.side == Side::A),
                commits.iter().any(|c| c.side == Side::B),
            );
            match (commits.len(), minimal, sides) {
                (_, false, _) => crate::i18n::message(
                    "repo-core-2814",
                    &[
                        ("budget", format!("{}", budget).into()),
                        ("count", (commits.len()).into()),
                    ],
                ),
                (1, true, _) => crate::i18n::text("repo-core-2815").to_owned(),
                (2, true, (true, true)) => crate::i18n::text("repo-core-2816").to_owned(),
                (n, true, _) => crate::i18n::message("repo-core-2817", &[("n", (n).into())]),
            }
        }
        Found::PassTogether => crate::i18n::message(
            "repo-core-2818",
            &[
                ("a", format!("{}", a).into()),
                ("b", format!("{}", b).into()),
            ],
        ),
        Found::FailsAlone(Side::A) => {
            crate::i18n::message("repo-core-2819", &[("a", format!("{}", a).into())])
        }
        Found::FailsAlone(Side::B) => {
            crate::i18n::message("repo-core-2820", &[("b", format!("{}", b).into())])
        }
        Found::DontApply => crate::i18n::message(
            "repo-core-2821",
            &[
                ("a", format!("{}", a).into()),
                ("b", format!("{}", b).into()),
            ],
        ),
        Found::Flaky => crate::i18n::text("repo-core-2822").to_owned(),
    }
}

/// The search's numbers: `12 tests run · 1 flaky result set aside`.
pub(crate) fn tally_line(i: &Interaction) -> String {
    let mut line = match i.tests {
        1 => crate::i18n::text("repo-core-2823").to_owned(),
        n => crate::i18n::message("repo-core-2824", &[("n", (n).into())]),
    };
    match i.flaky.len() {
        0 => {}
        1 => line.push_str(crate::i18n::text("repo-core-2825")),
        n => line.push_str(&crate::i18n::message(
            "repo-core-2826",
            &[("n", (n).into())],
        )),
    }
    line
}

/// Where a search is: `Test 7 of at most 40: 3 commits fail`.
pub(crate) fn progress_line(p: &SearchProgress) -> String {
    let probe = match p.probe {
        Probe::Pass => "pass",
        Probe::Fail => "fail",
        Probe::Unresolved => "unresolved",
    };
    crate::i18n::message(
        "repo-core-2831",
        &[
            ("tests", p.tests.into()),
            ("budget", p.budget.into()),
            ("size", p.size.into()),
            ("probe", probe.into()),
        ],
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
