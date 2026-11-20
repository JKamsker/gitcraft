//! `Plan::decode` of every step marker (#193): each well-formed shape its producer writes
//! decodes to its typed variant with the operands it carries, and every other operand count
//! decodes to `Step::Malformed` naming the marker, never to `Step::Git`.
use super::*;
use crate::gerrit::{AMEND, CHECK};
use crate::git::repository_tools::tracking::APPLY;
use crate::push_preflight::{APPROVED, FIX};
use crate::repo::forks::guard::{CHECKOUT_MARK, MARK};

fn kind(step: &[&str]) -> Step<'static> {
    // The decoded view borrows the argv; leak it so the test can compare by value.
    let argv: &'static [Vec<String>] = Box::leak(Box::new(ops(&[step])));
    Plan::decode(argv)
        .steps()
        .map(|s| s.kind)
        .next()
        .unwrap_or(Step::Git)
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

#[test]
fn expectations_decode_with_their_operands() {
    let condition = |step: &[&str]| match kind(step) {
        Step::Expectation(condition) => Some(condition),
        _ => None,
    };
    use Precondition as P;
    let cases: [(&[&str], P<'_>); 10] = [
        (
            &[EXPECT, "HEAD", "tip"],
            P::Ref {
                name: "HEAD",
                tip: "tip",
            },
        ),
        (
            &[EXPECT, "refs/stash", ""],
            P::Ref {
                name: "refs/stash",
                tip: "",
            },
        ),
        (
            &[EXPECT_SYMREF, "HEAD", "refs/heads/main"],
            P::Symref {
                name: "HEAD",
                target: "refs/heads/main",
                directory: None,
            },
        ),
        (
            &[EXPECT_SYMREF, "HEAD", "", "/linked"],
            P::Symref {
                name: "HEAD",
                target: "",
                directory: Some("/linked"),
            },
        ),
        (
            &[EXPECT_TREE, "stash@{0}^2", "tree"],
            P::Tree {
                revision: "stash@{0}^2",
                tree: "tree",
            },
        ),
        (&[EXPECT_HTTPS_ROUTE, "proof"], P::HttpsRoute("proof")),
        (&[EXPECT_FOLD_SAFE, "c0"], P::FoldSafe("c0")),
        (&[EXPECT_FOLD_IGNORED, "c0"], P::FoldIgnored("c0")),
        (
            &[CHECK, "base", "head"],
            P::GerritStack {
                base: "base",
                head: "head",
            },
        ),
        (
            &[MARK, "{\"name\":\"fork\"}"],
            P::ForkDestination("{\"name\":\"fork\"}"),
        ),
    ];
    for (step, expected) in cases {
        assert_eq!(condition(step), Some(expected), "{step:?}");
    }
    assert_eq!(
        condition(&[CHECKOUT_MARK, "{}"]),
        Some(P::ForkCheckout("{}"))
    );
}

#[test]
fn dispatched_steps_decode_with_their_operands() {
    let source = "gitcraft.commandSource=opaque";
    let evidence = strings(&["-c", source]);
    let (empty, fetch) = (strings(&[]), strings(&["fetch", "origin"]));
    let amend = strings(&["commit", "--amend", "-m", "Fix"]);
    let cases: [(&[&str], Step<'_>); 8] = [
        (
            &[TRY, "note", "fetch", "origin"],
            Step::Try {
                note: "note",
                args: &fetch,
            },
        ),
        (
            &[TRY, "note"],
            Step::Try {
                note: "note",
                args: &empty,
            },
        ),
        (
            &[APPROVED, "7"],
            Step::ReviewedPush {
                id: "7",
                evidence: None,
            },
        ),
        (
            &[APPROVED, "7", "-c", source],
            Step::ReviewedPush {
                id: "7",
                evidence: Some(&evidence),
            },
        ),
        (
            &[FIX, "8", "author"],
            Step::PreflightFix {
                id: "8",
                fix: "author",
            },
        ),
        (
            &[AMEND, "commit", "--amend", "-m", "Fix"],
            Step::GerritAmend { args: &amend },
        ),
        // `gerrit::amend_args` owns the amend's shape and refuses this one at dispatch.
        (&[AMEND], Step::GerritAmend { args: &empty }),
        (
            &[APPLY, "{\"branch\":\"topic\"}"],
            Step::TrackingApply {
                json: "{\"branch\":\"topic\"}",
            },
        ),
    ];
    for (step, expected) in cases {
        assert_eq!(kind(step), expected, "{step:?}");
    }
}

#[test]
fn every_marker_with_operands_no_producer_writes_is_malformed() {
    let counts: [(&str, &[usize]); 13] = [
        (EXPECT, &[0, 1, 3, 4, 5]),
        (EXPECT_SYMREF, &[0, 1, 4]),
        (EXPECT_TREE, &[0, 1, 3]),
        (EXPECT_HTTPS_ROUTE, &[0, 2]),
        (EXPECT_FOLD_SAFE, &[0, 2]),
        (EXPECT_FOLD_IGNORED, &[0, 2]),
        (CHECK, &[0, 1, 3]),
        (MARK, &[0, 2]),
        (CHECKOUT_MARK, &[0, 2]),
        (TRY, &[0]),
        // An id alone, or an id with `-c gitcraft.commandSource=…`, only.
        (APPROVED, &[0, 2, 3, 4]),
        (FIX, &[0, 1, 3]),
        (APPLY, &[0, 2, 3]),
    ];
    for (marker, wrong) in counts {
        for &count in wrong {
            let step: Vec<&str> = std::iter::once(marker)
                .chain(std::iter::repeat_n("x", count))
                .collect();
            let expected = Step::Malformed(Malformed::Operands { marker, count });
            assert_eq!(kind(&step), expected, "{marker:?} with {count} operands");
        }
    }
    let bad_option = [APPROVED, "7", "-x", "gitcraft.commandSource=opaque"];
    let bad_binding = [APPROVED, "7", "-c", "core.pager=cat"];
    for step in [&bad_option, &bad_binding] {
        let expected = Malformed::Operands {
            marker: APPROVED,
            count: 3,
        };
        assert_eq!(kind(step), Step::Malformed(expected), "{step:?}");
    }
}

#[test]
fn unknown_nul_markers_are_malformed_and_git_words_stay_git() {
    for marker in ["\0ghost-merge", "\0wip", "\0parked:1", "\0brand-new", "\0"] {
        let expected = Step::Malformed(Malformed::UnknownMarker(marker));
        assert_eq!(kind(&[marker]), expected);
        assert_eq!(kind(&[marker, "x", "y"]), expected);
    }
    let lease = "--force-with-lease=refs/heads/t:\0fork-pushed-tip";
    for step in [&["fetch"][..], &["push", lease, "--", "fork", "a:b"], &[]] {
        assert_eq!(kind(step), Step::Git, "{step:?}");
    }
    // A marker only counts as the first word.
    assert_eq!(kind(&["commit", "-m", TRY]), Step::Git);
}
