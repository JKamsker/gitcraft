//! The typed decode leaves every executor branch as it was (#193). `legacy` is `Plan::decode`
//! as it was before the typed variants; `Execution::step`/`dispatch` branched on its classes
//! (and on `[APPLY, json]` by hand). This does not run the executor: it checks, over every
//! marker with 0..=5 operands (repeated and distinct words) and every v1 fixture step, that each
//! new kind selects the branch the legacy class selected, with the operands it destructured:
//! - an expectation is skipped, and is one exactly when `is_expectation` says so;
//! - TRY, FIX and AMEND reach the same branch with the same operands;
//! - APPROVED reaches the reviewed dispatch with `reviewed_operands`' operands when it accepts
//!   them; otherwise it is `Malformed`, and `prepare_command` refuses it through
//!   `reviewed_push_args` before any dispatch, as before;
//! - `TrackingApply` is exactly the `[APPLY, json]` argv the executor matched by hand;
//! - `Malformed` is a legacy `Git` step that starts with a marker, and still runs as Git.
use super::*;
use crate::gerrit::{AMEND, CHECK};
use crate::git::repository_tools::tracking::APPLY;
use crate::push_preflight::{APPROVED, FIX};
use crate::repo::timeline::store::load_from;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Legacy {
    Expectation,
    Try,
    ReviewedPush,
    PreflightFix,
    GerritAmend,
    Git,
}

fn legacy(argv: &[String]) -> Legacy {
    match argv {
        [mark, _, ..] if mark == TRY => Legacy::Try,
        _ if is_expectation(argv) => Legacy::Expectation,
        [mark, ..] if mark == APPROVED => Legacy::ReviewedPush,
        [mark, _, _] if mark == FIX => Legacy::PreflightFix,
        [mark, ..] if mark == AMEND => Legacy::GerritAmend,
        _ => Legacy::Git,
    }
}

fn routes_as_before(argv: &[String], kind: Step<'_>) -> bool {
    match (legacy(argv), kind) {
        (Legacy::Expectation, Step::Expectation(_)) => true,
        (Legacy::Try, Step::Try { note, args }) => argv[1] == note && argv[2..] == *args,
        (Legacy::PreflightFix, Step::PreflightFix { id, fix }) => argv[1..] == [id, fix],
        (Legacy::GerritAmend, Step::GerritAmend { args }) => argv[1..] == *args,
        (Legacy::ReviewedPush, Step::ReviewedPush { id, evidence }) => {
            reviewed_operands(argv) == Ok((id, evidence))
        }
        // Refused before the registry is consulted: no registered plan can make it run.
        (Legacy::ReviewedPush, Step::Malformed(_)) => {
            reviewed_operands(argv).is_err() && reviewed_push_args(argv).is_err()
        }
        (Legacy::Git, Step::TrackingApply { json }) => {
            matches!(argv, [mark, saved] if mark == APPLY && saved == json)
        }
        (Legacy::Git, Step::Git) => !starts_with_marker(argv),
        (Legacy::Git, Step::Malformed(_)) => {
            starts_with_marker(argv) && !matches!(argv, [mark, _] if mark == APPLY)
        }
        _ => false,
    }
}

/// A NUL-prefixed first word, or the one non-NUL marker.
fn starts_with_marker(argv: &[String]) -> bool {
    argv.first()
        .is_some_and(|w| w.starts_with('\0') || w == EXPECT_HTTPS_ROUTE)
}

fn corpus() -> Result<Vec<Vec<String>>, String> {
    let mut markers: Vec<&str> = super::super::sentinel_inventory::INVENTORY
        .iter()
        .map(|s| s.value)
        .collect();
    markers.extend(["fetch", "\0unknown"]);
    let words = ["x", "-c", "gitcraft.commandSource=x", "commit"];
    let mut steps = vec![Vec::new()];
    for marker in markers {
        for count in 0..=5 {
            for word in words {
                let operands = std::iter::repeat_n(word, count);
                let step = std::iter::once(marker).chain(operands);
                steps.push(step.map(str::to_owned).collect());
            }
            let distinct = (0..count).map(|i| format!("w{i}"));
            steps.push(std::iter::once(marker.to_owned()).chain(distinct).collect());
        }
        let reviewed = [marker, "7", "-c", "gitcraft.commandSource=x"];
        steps.push(reviewed.map(str::to_owned).to_vec());
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/repo/operation_plan/fixtures/v1");
    let saved = load_from(&dir.join("timeline.json"))?;
    for undo in saved.entries.iter().filter_map(|e| e.undo.as_ref()) {
        steps.extend(undo.undo.iter().chain(&undo.redo).cloned());
    }
    let live =
        std::fs::read_to_string(dir.join("live_only_steps.json")).map_err(|e| e.to_string())?;
    let live: Vec<Vec<String>> = serde_json::from_str(&live).map_err(|e| e.to_string())?;
    steps.extend(live);
    Ok(steps)
}

#[test]
fn every_step_routes_through_the_executor_as_the_legacy_decode_did() -> Result<(), String> {
    let steps = corpus()?;
    let plan = Plan::decode(&steps);
    for step in plan.steps() {
        let argv = step.argv;
        assert!(
            routes_as_before(argv, step.kind),
            "{argv:?} decoded as {:?}",
            step.kind
        );
        let skipped = matches!(step.kind, Step::Expectation(_));
        assert_eq!(skipped, is_expectation(argv), "{argv:?}");
    }
    assert!(plan.steps().zip(&steps).all(|(s, argv)| s.argv == argv));
    Ok(())
}

#[test]
fn gerrit_stacks_are_exactly_the_check_steps_in_saved_order() -> Result<(), String> {
    let steps = corpus()?;
    let by_hand: Vec<(&str, &str)> = steps
        .iter()
        .filter_map(|op| match op.as_slice() {
            [mark, base, head] if mark == CHECK => Some((base.as_str(), head.as_str())),
            _ => None,
        })
        .collect();
    assert!(!by_hand.is_empty());
    assert_eq!(
        Plan::decode(&steps).gerrit_stacks().collect::<Vec<_>>(),
        by_hand
    );
    Ok(())
}
