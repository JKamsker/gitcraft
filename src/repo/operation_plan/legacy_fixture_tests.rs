//! Version-1 persistence fixtures (#193) in `fixtures/v1/`, shaped the way each producer emits
//! its steps. `timeline.json` and `live_only_steps.json` are canonical (`store::save_to`
//! writes exactly `serde_json::to_string`), so they must round-trip byte for byte.
//! `timeline_legacy_fields.json` predates fields that now default; it decodes and round-trips
//! semantically, and serializing it writes the defaults out, so its bytes cannot be stable.
//! `undo_entry_mixed_try.json` is synthetic: no current producer persists a `TRY` (issue start
//! runs it unrecorded), but it keeps the mixed optional/required redo the pre-#200 model test
//! decoded, in the same legacy shape, built from `issue_work::link::start_ops` steps.

use super::sentinel_inventory::{HOW_TO, INVENTORY, Role, classify_all};
use super::*;
use crate::repo::timeline::store::{Saved, load_from};
use std::path::{Path, PathBuf};

const R: &str = "Expectation::Ref";
const S: &str = "Expectation::Symref";
const T: &str = "Expectation::Tree";
const G: &str = "Git";
const GUARD: &str = "Expectation::FeatureGuard";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/repo/operation_plan/fixtures/v1")
        .join(name)
}

/// The fixture's text as checked in: one line and a newline (`.gitattributes` pins LF).
fn text(name: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(fixture(name)).map_err(|e| format!("{name}: {e}"))?;
    Ok(text.trim_end_matches(['\r', '\n']).to_owned())
}

fn timeline(name: &str) -> Result<Saved, String> {
    load_from(&fixture(name))
}

fn undo_of(saved: &Saved, id: u64) -> Result<&UndoEntry, String> {
    let entry = saved.entries.iter().find(|e| e.id == id);
    entry
        .and_then(|e| e.undo.as_ref())
        .ok_or(format!("entry {id} has no undo"))
}

#[test]
fn canonical_fixtures_round_trip_byte_for_byte() -> Result<(), String> {
    let saved = timeline("timeline.json")?;
    let again = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
    assert_eq!(again, text("timeline.json")?);
    let live: Vec<Vec<String>> =
        serde_json::from_str(&text("live_only_steps.json")?).map_err(|e| e.to_string())?;
    let again = serde_json::to_string(&live).map_err(|e| e.to_string())?;
    assert_eq!(again, text("live_only_steps.json")?);
    // Still the format this build writes: bumping it needs v2 fixtures beside these.
    let current = Saved::of(&crate::repo::timeline::Timeline::default(), Path::new("/x"));
    assert_eq!((saved.version, current.version), (1, 1));
    Ok(())
}

#[test]
fn records_missing_later_fields_decode_with_defaults_and_round_trip_semantically()
-> Result<(), String> {
    let saved = timeline("timeline_legacy_fields.json")?;
    assert_eq!((saved.folder.as_str(), saved.root_redo), ("", None));
    let first = &saved.entries[0];
    assert_eq!(
        (first.files.len(), first.files_total, first.redo_child),
        (0, 0, None)
    );
    assert!(first.receipt.is_none() && saved.entries[2].undo.is_none());
    let (commit, delete) = (undo_of(&saved, 1)?, undo_of(&saved, 2)?);
    assert_eq!((commit.id, commit.agent.as_deref()), (0, None));
    assert_eq!(commit.heads, Some(("a0".repeat(20), "a1".repeat(20))));
    assert_eq!((delete.id, delete.heads.as_ref()), (0, None));
    let written = serde_json::to_string(&saved).map_err(|e| e.to_string())?;
    assert_ne!(
        written,
        text("timeline_legacy_fields.json")?,
        "defaults are written out"
    );
    let reread: Saved = serde_json::from_str(&written).map_err(|e| e.to_string())?;
    assert_eq!(reread, saved);

    let mixed: UndoEntry =
        serde_json::from_str(&text("undo_entry_mixed_try.json")?).map_err(|e| e.to_string())?;
    assert_eq!(
        (mixed.id, mixed.agent.as_deref(), mixed.heads.as_ref()),
        (0, None, None)
    );
    let written = serde_json::to_string(&mixed).map_err(|e| e.to_string())?;
    let reread: UndoEntry = serde_json::from_str(&written).map_err(|e| e.to_string())?;
    assert_eq!(reread, mixed);
    Ok(())
}

#[test]
fn persisted_steps_classify_to_the_expected_typed_plan() -> Result<(), String> {
    let saved = timeline("timeline.json")?;
    let expected: [(u64, &[&str], &[&str]); 6] = [
        (1, &[S, G], &[S, G]),
        (2, &[R, T, T, G, G, G, G, G], &[G]),
        (3, &[R, S, R, S, G, G, G, G, G], &[R, S, R, S, G, G, G, G]),
        (4, &[GUARD, G, R, G, G], &[GUARD, G, G, G, G, G]),
        (5, &[GUARD, S, R, R, G, G], &[GUARD, S, R, G, G]),
        (6, &[G], &[G]),
    ];
    for (id, undo, redo) in expected {
        let entry = undo_of(&saved, id)?;
        assert_eq!(classify_all(&entry.undo), undo, "undo of entry {id}");
        assert_eq!(classify_all(&entry.redo), redo, "redo of entry {id}");
    }
    let legacy = timeline("timeline_legacy_fields.json")?;
    assert_eq!(classify_all(&undo_of(&legacy, 2)?.redo), [R, G]);
    let live: Vec<Vec<String>> =
        serde_json::from_str(&text("live_only_steps.json")?).map_err(|e| e.to_string())?;
    let kinds = [
        "Expectation::HttpsRoute",
        GUARD,
        G,
        "GerritAmend",
        "Expectation::FoldSafe",
        G,
        "Expectation::FoldIgnored",
        "ReviewedPush",
        "PreflightFix",
        "Try",
        G,
    ];
    assert_eq!(classify_all(&live), kinds);
    Ok(())
}

#[test]
fn a_mixed_optional_and_required_redo_keeps_its_retry_semantics() -> Result<(), String> {
    let entry: UndoEntry =
        serde_json::from_str(&text("undo_entry_mixed_try.json")?).map_err(|e| e.to_string())?;
    assert_eq!(classify_all(&entry.redo), [R, "Try", G, G]);
    let plan = model::Plan::decode(&entry.redo);
    let tried = plan.steps().nth(1).map(|s| s.kind);
    assert!(
        matches!(tried, Some(model::Step::Try { args, .. }) if args == ["fetch", "origin", "main"])
    );
    let mut retry = entry.redo.clone();
    retain_unfinished(&mut retry, 2);
    let kept = [&entry.redo[0], &entry.redo[2], &entry.redo[3]];
    assert_eq!(retry.iter().collect::<Vec<_>>(), kept);
    Ok(())
}

fn words(ops: &[Vec<String>]) -> impl Iterator<Item = &String> {
    ops.iter().flatten()
}

#[test]
fn the_fixtures_cover_every_step_sentinel_and_hold_no_unknown_one() -> Result<(), String> {
    let saved = timeline("timeline.json")?;
    let persisted: Vec<&String> = saved
        .entries
        .iter()
        .filter_map(|e| e.undo.as_ref())
        .flat_map(|u| words(&u.undo).chain(words(&u.redo)))
        .collect();
    let live: Vec<Vec<String>> =
        serde_json::from_str(&text("live_only_steps.json")?).map_err(|e| e.to_string())?;
    let live: Vec<&String> = words(&live).collect();
    for sentinel in INVENTORY
        .iter()
        .filter(|s| matches!(s.role, Role::Step | Role::Placeholder))
    {
        let held = |w: &&String| match sentinel.role {
            Role::Placeholder => w.contains(sentinel.value),
            _ => *w == sentinel.value,
        };
        let (name, saved, unsaved) = (
            sentinel.constant,
            persisted.iter().any(held),
            live.iter().any(held),
        );
        // Persisted ones are in the timeline file, the others only in the live steps.
        assert_eq!(
            (saved, unsaved),
            (sentinel.persisted, !sentinel.persisted),
            "{name}. {HOW_TO}"
        );
    }
    let known = |w: &&&String| INVENTORY.iter().any(|s| w.contains(s.value));
    let unknown: Vec<_> = persisted
        .iter()
        .chain(&live)
        .filter(|w| w.contains('\0'))
        .filter(|w| !known(w))
        .collect();
    assert!(
        unknown.is_empty(),
        "uninventoried sentinels in fixtures: {unknown:?}"
    );
    Ok(())
}

#[test]
fn fixture_steps_are_what_the_producers_build() -> Result<(), String> {
    let saved = timeline("timeline.json")?;
    let (a0, a1) = ("a0".repeat(20), "a1".repeat(20));
    let heads = format!("{a0}\0{a1}\0refs/heads/main");
    let kind = OpKind::Normal(Undo::HeadMove { mode: "--soft" }, heads);
    let mut commit = undo_entry("Commit", &kind).ok_or("HeadMove gave no undo")?;
    commit.id = 1;
    assert_eq!(&commit, undo_of(&saved, 1)?);
    let stash = undo_of(&saved, 2)?;
    assert_eq!(stash.undo[0][..2], expect_ref("refs/stash")[..2]);
    assert_eq!(stash.undo[2], expect_tree("stash@{0}^2", &"72".repeat(20)));
    let stack = undo_of(&saved, 3)?;
    assert_eq!(
        stack.undo[0],
        expect_ref_at("refs/heads/feature/a", &"c1".repeat(20))
    );
    let worktree = expect_symref_in("/home/dev/src/app-b", "HEAD", "refs/heads/feature/b");
    assert_eq!(stack.undo[3], worktree);
    let review = crate::gerrit::review_ops("origin", "main", "fix", &a1, &a0)?;
    let live: Vec<Vec<String>> =
        serde_json::from_str(&text("live_only_steps.json")?).map_err(|e| e.to_string())?;
    assert_eq!(live[1..3], review[..]);
    let entry: UndoEntry =
        serde_json::from_str(&text("undo_entry_mixed_try.json")?).map_err(|e| e.to_string())?;
    let says = "couldn't fetch origin, so it starts from origin/main as last fetched";
    assert_eq!(entry.redo[1], try_step(&["fetch", "origin", "main"], says));
    assert_eq!(live[9], entry.redo[1]);
    Ok(())
}
