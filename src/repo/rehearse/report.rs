//! What a rehearsal's report says: where it stops or what it gives, the test, and how two
//! rehearsals compare.

use crate::git::{Played, Rehearsal, RehearseKind, Tested, Verdict};
use crate::repo::short;

/// The operation, as a heading: `Rebase feature onto main`, `Merge main into feature`.
pub(crate) fn heading(kind: RehearseKind, branch: &str, other: &str) -> String {
    match kind {
        RehearseKind::Rebase => crate::i18n::message(
            "repo-workflows-rehearse-heading-52968e32",
            &[
                ("branch", (branch).clone().into()),
                ("other", (other).clone().into()),
            ],
        ),
        RehearseKind::Merge => crate::i18n::message(
            "repo-workflows-rehearse-heading-e812253e",
            &[
                ("other", (other).clone().into()),
                ("branch", (branch).clone().into()),
            ],
        ),
    }
}

/// How the `kind` went: `Stops at commit 3 of 5 (1a2b3c4d Parse headers): 2 conflicts`.
pub(crate) fn played_line(kind: RehearseKind, played: &Played) -> String {
    match played {
        Played::UpToDate => {
            crate::i18n::text("repo-workflows-rehearse-played-line-357eba9b").to_owned()
        }
        Played::FastForward { tip, .. } => crate::i18n::message(
            "repo-workflows-rehearse-played-line-78e91cc7",
            &[("short-tip-1", (short(tip)).clone().into())],
        ),
        Played::Clean { commits, .. } if kind == RehearseKind::Merge => match commits {
            1 => crate::i18n::text("repo-workflows-rehearse-played-line-6b8c28e6").to_owned(),
            n => crate::i18n::message(
                "repo-workflows-rehearse-played-line-cc3a8109",
                &[("n", (n).clone().into())],
            ),
        },
        Played::Clean { commits, .. } => match commits {
            0 => crate::i18n::text("repo-workflows-rehearse-played-line-a969dd77").to_owned(),
            1 => crate::i18n::text("repo-workflows-rehearse-played-line-31877584").to_owned(),
            n => crate::i18n::message(
                "repo-workflows-rehearse-played-line-e270e3ca",
                &[("n", (n).clone().into())],
            ),
        },
        Played::Stopped {
            step,
            of,
            commit,
            summary,
            conflicts,
        } => {
            let files = match conflicts.len() {
                1 => crate::i18n::text("repo-workflows-rehearse-played-line-2704eda1").to_owned(),
                n => crate::i18n::message(
                    "repo-workflows-rehearse-played-line-3fb21e29",
                    &[("n", (n).clone().into())],
                ),
            };
            match commit {
                Some(commit) => {
                    let what = if summary.is_empty() {
                        short(commit).to_owned()
                    } else {
                        format!("{} {summary}", short(commit))
                    };
                    crate::i18n::message(
                        "repo-workflows-rehearse-played-line-9a2020a6",
                        &[
                            ("step", (step).clone().into()),
                            ("of", (of).clone().into()),
                            ("what", (what).clone().into()),
                            ("files", (files).clone().into()),
                        ],
                    )
                }
                None => crate::i18n::message(
                    "repo-workflows-rehearse-played-line-6fc15496",
                    &[("files", (files).clone().into())],
                ),
            }
        }
        Played::Failed(e) => crate::i18n::message(
            "repo-workflows-rehearse-played-line-da976bc1",
            &[("e", (e).clone().into())],
        ),
    }
}

/// What the test said: `` `cargo test` fails (exit code 101) ``.
pub(crate) fn tested_line(tested: &Tested, command: &str) -> String {
    match tested {
        Tested::Ran {
            verdict: Verdict::Good,
            ..
        } => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-4deabe28",
            &[("command", (command).clone().into())],
        ),
        Tested::Ran {
            verdict: Verdict::Bad,
            code: Some(code),
            ..
        } => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-437947b9",
            &[
                ("command", (command).clone().into()),
                ("code", (code).clone().into()),
            ],
        ),
        Tested::Ran {
            verdict: Verdict::Bad,
            code: None,
            ..
        } => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-ebe78b3a",
            &[("command", (command).clone().into())],
        ),
        Tested::Known(Verdict::Good) => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-ff964b6a",
            &[("command", (command).clone().into())],
        ),
        Tested::Known(Verdict::Bad) => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-26bf178a",
            &[("command", (command).clone().into())],
        ),
        Tested::NotRun(why) => crate::i18n::message(
            "repo-workflows-rehearse-tested-line-246783da",
            &[
                ("command", (command).clone().into()),
                ("why", (why).clone().into()),
            ],
        ),
    }
}

/// How two rehearsals of the same branch compare, when both went through.
pub(crate) fn compare_line(a: &Rehearsal, b: &Rehearsal) -> Option<String> {
    let (ta, tb) = (a.played.tree()?, b.played.tree()?);
    Some(if ta == tb {
        crate::i18n::text("repo-workflows-rehearse-compare-line-16ca6f02").to_owned()
    } else {
        crate::i18n::text("repo-workflows-rehearse-compare-line-cd991a2e").to_owned()
    })
}

/// Whether it went through and its test (if any) didn't fail: shown as good news.
pub(crate) fn is_good(r: &Rehearsal) -> bool {
    let went = matches!(
        r.played,
        Played::UpToDate | Played::FastForward { .. } | Played::Clean { .. }
    );
    let failed = matches!(
        r.tested,
        Some(Tested::Ran {
            verdict: Verdict::Bad,
            ..
        }) | Some(Tested::Known(Verdict::Bad))
    );
    went && !failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::RehearseSpec;

    #[test]
    fn a_stop_names_the_commit_and_counts_the_conflicts() {
        let stopped = Played::Stopped {
            step: 7,
            of: 12,
            commit: Some("0123456789abcdef".to_owned()),
            summary: "Parse headers".to_owned(),
            conflicts: vec!["a".into(), "b".into()],
        };
        assert_eq!(
            played_line(RehearseKind::Rebase, &stopped),
            "Stops at commit 7 of 12 (01234567 Parse headers): 2 conflicts"
        );
        let merge = Played::Stopped {
            step: 1,
            of: 1,
            commit: None,
            summary: String::new(),
            conflicts: vec!["ü x.txt".into()],
        };
        assert_eq!(
            played_line(RehearseKind::Merge, &merge),
            "Stops with 1 conflict"
        );
        assert_eq!(
            played_line(
                RehearseKind::Merge,
                &Played::Clean {
                    tip: "t".into(),
                    tree: "x".into(),
                    commits: 4
                }
            ),
            "Goes through cleanly: a merge commit, bringing in 4 commits"
        );
    }

    #[test]
    fn clean_results_read_plainly() {
        let clean = |commits| Played::Clean {
            tip: "t".into(),
            tree: "x".into(),
            commits,
        };
        assert_eq!(
            played_line(RehearseKind::Rebase, &clean(1)),
            "Goes through cleanly: 1 commit"
        );
        assert_eq!(
            played_line(RehearseKind::Rebase, &clean(12)),
            "Goes through cleanly: 12 commits"
        );
        assert!(played_line(RehearseKind::Rebase, &clean(0)).contains("no commit to replay"));
        assert!(played_line(RehearseKind::Rebase, &Played::UpToDate).starts_with("Nothing to do"));
        assert_eq!(
            played_line(RehearseKind::Rebase, &Played::Failed("hook".into())),
            "Fails: hook"
        );
    }

    #[test]
    fn tests_read_with_their_command() {
        let ran = |verdict, code| Tested::Ran {
            verdict,
            code,
            lines: Vec::new(),
        };
        assert_eq!(
            tested_line(&ran(Verdict::Bad, Some(101)), "cargo test"),
            "`cargo test` fails (exit code 101)"
        );
        assert_eq!(
            tested_line(&ran(Verdict::Good, Some(0)), "make"),
            "`make` passes"
        );
        assert_eq!(
            tested_line(&Tested::NotRun("it stops on conflicts".into()), "make"),
            "`make` not run: it stops on conflicts"
        );
    }

    #[test]
    fn two_rehearsals_compare_by_their_files() {
        let r = |kind, tree: &str| Rehearsal {
            spec: RehearseSpec {
                kind,
                branch: "refs/heads/f".into(),
                other: "refs/heads/main".into(),
                command: None,
            },
            branch_tip: "a".into(),
            other_tip: "b".into(),
            played: Played::Clean {
                tip: "c".into(),
                tree: tree.into(),
                commits: 1,
            },
            tested: None,
        };
        let (a, b) = (r(RehearseKind::Rebase, "t1"), r(RehearseKind::Merge, "t1"));
        assert_eq!(
            compare_line(&a, &b).as_deref(),
            Some("Both give the same files.")
        );
        let c = r(RehearseKind::Merge, "t2");
        assert_eq!(
            compare_line(&a, &c).as_deref(),
            Some("They give different files.")
        );
        assert!(is_good(&a));
        assert_eq!(
            heading(RehearseKind::Merge, "feature", "main"),
            "Merge main into feature"
        );
    }
}
