//! `Plan::decode` for one saved step: exhaustive over every internal step marker. A known
//! marker with operands its producer never writes, and any other NUL-prefixed first word,
//! decode to `Step::Malformed`, never to `Step::Git`. Only a step that starts with a Git word
//! is `Git` (a placeholder such as `PUSHED_TIP` stands inside a Git argument, not first).
use super::*;
use crate::gerrit::{AMEND, CHECK};
use crate::git::repository_tools::tracking::APPLY;
use crate::push_preflight::{APPROVED, FIX};
use crate::repo::forks::guard::guard_of;

/// Markers of steps that run nothing and only check (`is_expectation` agrees on their shapes).
/// The fork guards' markers stay private to `forks::guard`; `guard_of` recognizes them.
const EXPECTATIONS: [&str; 7] = [
    EXPECT,
    EXPECT_SYMREF,
    EXPECT_TREE,
    EXPECT_HTTPS_ROUTE,
    EXPECT_FOLD_SAFE,
    EXPECT_FOLD_IGNORED,
    CHECK,
];

/// Markers of steps gitcraft runs itself, or runs as Git differently.
const DISPATCHED: [&str; 5] = [TRY, APPROVED, FIX, AMEND, APPLY];

pub(super) fn step(argv: &[String]) -> Step<'_> {
    let Some((marker, operands)) = argv.split_first() else {
        return Step::Git;
    };
    if let Some(condition) = precondition(argv) {
        return Step::Expectation(condition);
    }
    if let Some(kind) = dispatched(marker, operands, argv) {
        return kind;
    }
    let marker = marker.as_str();
    let fork_guard = guard_of(argv).is_some();
    if EXPECTATIONS.contains(&marker) || DISPATCHED.contains(&marker) || fork_guard {
        let count = operands.len();
        Step::Malformed(Malformed::Operands { marker, count })
    } else if marker.starts_with('\0') {
        Step::Malformed(Malformed::UnknownMarker(marker))
    } else {
        Step::Git
    }
}

fn precondition(argv: &[String]) -> Option<Precondition<'_>> {
    if let Some((name, tip)) = expected_ref(argv) {
        return Some(Precondition::Ref { name, tip });
    }
    if let Some((name, target, directory)) = expected_symref(argv) {
        return Some(Precondition::Symref {
            name,
            target,
            directory,
        });
    }
    if let Some((revision, tree)) = expected_tree(argv) {
        return Some(Precondition::Tree { revision, tree });
    }
    Some(match argv {
        [mark, proof] if mark == EXPECT_HTTPS_ROUTE => Precondition::HttpsRoute(proof),
        [mark, target] if mark == EXPECT_FOLD_SAFE => Precondition::FoldSafe(target),
        [mark, target] if mark == EXPECT_FOLD_IGNORED => Precondition::FoldIgnored(target),
        [mark, base, head] if mark == CHECK => Precondition::GerritStack { base, head },
        _ => return fork_guard(argv),
    })
}

fn fork_guard(argv: &[String]) -> Option<Precondition<'_>> {
    match guard_of(argv)? {
        (false, [json]) => Some(Precondition::ForkDestination(json)),
        (true, [json]) => Some(Precondition::ForkCheckout(json)),
        _ => None,
    }
}

fn dispatched<'a>(marker: &str, operands: &'a [String], argv: &'a [String]) -> Option<Step<'a>> {
    match (marker, operands) {
        (TRY, [note, args @ ..]) => Some(Step::Try { note, args }),
        // The executor's own strict reader, so decode and dispatch cannot disagree.
        (APPROVED, _) => reviewed_operands(argv)
            .ok()
            .map(|(id, evidence)| Step::ReviewedPush { id, evidence }),
        (FIX, [id, fix]) => Some(Step::PreflightFix { id, fix }),
        (AMEND, args) => Some(Step::GerritAmend { args }),
        (APPLY, [json]) => Some(Step::TrackingApply { json }),
        _ => None,
    }
}
