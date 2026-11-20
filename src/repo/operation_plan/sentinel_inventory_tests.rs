//! The checked-in inventory of gitcraft's internal sentinels (#193): every NUL-prefixed
//! string constant in production source, plus the one non-NUL step marker, with its owner,
//! what it is, whether a current producer persists it in a timeline `UndoEntry`, and how
//! `Plan::decode` classifies a step built the way its producer builds it. The table is in
//! `sentinel_table_tests.rs`; docs/architecture.md ("Operation sentinel inventory") mirrors it.

use super::model::{Plan, Precondition, Step};
use std::collections::BTreeSet;
use std::path::Path;

#[path = "sentinel_scan_tests.rs"]
mod scan;
#[path = "sentinel_table_tests.rs"]
mod table;
pub(super) use table::{INVENTORY, Role};

/// `Plan::decode`'s classification of one step, as the inventory and fixtures name it.
pub(super) fn classify(kind: &Step<'_>) -> &'static str {
    match kind {
        Step::Expectation(condition) => match condition {
            Precondition::Ref { .. } => "Expectation::Ref",
            Precondition::Symref { .. } => "Expectation::Symref",
            Precondition::Tree { .. } => "Expectation::Tree",
            Precondition::HttpsRoute(_) => "Expectation::HttpsRoute",
            Precondition::FoldSafe(_) => "Expectation::FoldSafe",
            Precondition::FoldIgnored(_) => "Expectation::FoldIgnored",
            Precondition::FeatureGuard => "Expectation::FeatureGuard",
        },
        Step::Try { .. } => "Try",
        Step::ReviewedPush => "ReviewedPush",
        Step::PreflightFix => "PreflightFix",
        Step::GerritAmend => "GerritAmend",
        Step::Git => "Git",
    }
}

/// The classification of each step of `ops`, in order.
pub(super) fn classify_all(ops: &[Vec<String>]) -> Vec<&'static str> {
    Plan::decode(ops)
        .steps()
        .map(|s| classify(&s.kind))
        .collect()
}

pub(super) const HOW_TO: &str = "For each new sentinel add an entry to INVENTORY in \
src/repo/operation_plan/sentinel_table_tests.rs (constant, owner file, value, role, whether a \
producer persists it, a producer-shaped sample and its Plan::decode classification) and a row to \
the \"Operation sentinel inventory\" table in docs/architecture.md. If it marks an operation step, \
teach Plan::decode (src/repo/operation_plan/model.rs) to classify it rather than fall through to \
Step::Git, and if a producer persists it in an UndoEntry, add it to a fixture under \
src/repo/operation_plan/fixtures/v1/.";

#[test]
fn every_nul_sentinel_in_production_source_is_in_the_inventory() -> Result<(), String> {
    let found = scan::scan(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let declared: BTreeSet<_> = found
        .iter()
        .filter_map(|f| Some((f.file.as_str(), f.constant.as_deref()?, f.value.as_str())))
        .collect();
    let listed: BTreeSet<_> = INVENTORY
        .iter()
        .filter(|s| s.value.starts_with('\0'))
        .map(|s| (s.owner, s.constant, s.value))
        .collect();
    let new: Vec<_> = declared
        .difference(&listed)
        .map(|d| format!("  {d:?}"))
        .collect();
    let gone: Vec<_> = listed
        .difference(&declared)
        .map(|d| format!("  {d:?}"))
        .collect();
    let values: BTreeSet<_> = INVENTORY.iter().map(|s| s.value).collect();
    let inline: Vec<_> = found
        .iter()
        .filter(|f| f.constant.is_none() && !values.contains(f.value.as_str()))
        .map(|f| {
            format!(
                "  {}:{} {:?} (declare it as a constant)",
                f.file, f.line, f.value
            )
        })
        .collect();
    assert!(
        new.is_empty() && gone.is_empty() && inline.is_empty(),
        "The operation sentinel inventory is out of date.\nNew (file, constant, value):\n{}\n\
         Inline NUL-prefixed literals with no inventoried value:\n{}\n\
         Listed but no longer declared (remove them):\n{}\n{HOW_TO}",
        new.join("\n"),
        inline.join("\n"),
        gone.join("\n"),
    );
    Ok(())
}

#[test]
fn every_step_sentinel_decodes_as_documented() {
    for sentinel in INVENTORY.iter().filter(|s| !s.sample.is_empty()) {
        let got = classify_all(&[sentinel.sample_step()]);
        let (name, owner) = (sentinel.constant, sentinel.owner);
        assert_eq!(got, [sentinel.decodes], "{name} ({owner}): {HOW_TO}");
    }
    for sentinel in INVENTORY {
        let name = sentinel.constant;
        let is_step = matches!(sentinel.role, Role::Step | Role::Placeholder);
        assert_eq!(
            !sentinel.sample.is_empty(),
            is_step,
            "{name}: only steps have samples"
        );
        let unclassified = sentinel.role == Role::Step && sentinel.decodes == "Git";
        assert_eq!(
            unclassified,
            !sentinel.handled_by.is_empty(),
            "{name}: a step marker Plan::decode leaves as Step::Git must say what handles it. \
             {HOW_TO}"
        );
    }
}

#[test]
fn inventoried_values_are_the_constants_the_code_uses() {
    use crate::repo::operations as o;
    let visible = [
        ("EXPECT", o::EXPECT),
        ("EXPECT_SYMREF", o::EXPECT_SYMREF),
        ("EXPECT_TREE", o::EXPECT_TREE),
        ("TRY", o::TRY),
        ("TRIED", o::TRIED),
        ("EXPECT_FOLD_SAFE", o::EXPECT_FOLD_SAFE),
        ("EXPECT_FOLD_IGNORED", o::EXPECT_FOLD_IGNORED),
        ("EXPECT_HTTPS_ROUTE", super::EXPECT_HTTPS_ROUTE),
        ("CHECK", crate::gerrit::CHECK),
        ("AMEND", crate::gerrit::AMEND),
        ("APPROVED", crate::push_preflight::APPROVED),
        ("FIX", crate::push_preflight::FIX),
        ("APPLY", crate::git::repository_tools::tracking::APPLY),
        ("WIP_HASH", crate::repo::WIP_HASH),
    ];
    for (constant, value) in visible {
        let listed = INVENTORY
            .iter()
            .find(|s| s.constant == constant)
            .map(|s| s.value);
        assert_eq!(listed, Some(value), "{constant}");
    }
}

#[test]
fn the_architecture_doc_lists_every_inventoried_sentinel() -> Result<(), String> {
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/architecture.md");
    let text = std::fs::read_to_string(&doc).map_err(|e| e.to_string())?;
    for sentinel in INVENTORY {
        let start = format!("| `{}` | `{}` |", sentinel.constant, sentinel.owner);
        let row = text.lines().find(|l| l.starts_with(&start));
        let row = row.ok_or(format!("docs/architecture.md lacks a row starting {start}"))?;
        let persisted = if sentinel.persisted { "| yes" } else { "| no" };
        let decodes = format!("`{}`", sentinel.decodes);
        let decoded = sentinel.decodes.is_empty() || row.contains(&decodes);
        assert!(
            row.contains(persisted) && decoded,
            "stale row in docs/architecture.md: {row}"
        );
    }
    Ok(())
}
