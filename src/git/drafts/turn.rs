//! Strict inverse-turn application. Later non-overlapping work is retained; an overlap
//! fails before writes. Raw private Git context preserves the real index and bypasses
//! checkout filters. A protected safety draft is retained for recovery/undo.
use super::*;
use std::sync::Arc;

pub fn review(
    repo: &Path,
    parent: &str,
    turn: &str,
    session: &str,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    if !oid_valid(parent) || !oid_valid(turn) {
        return Err(crate::i18n::text("backend-turn-has-no-immutable-parent-3272f8").into());
    }
    let ctx = Context::new(repo, active)?;
    let (reference, old, state) = load_review(&ctx, session, active)?;
    let known = state.known;
    let stat = ctx.run(
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if stat.len() > 128 * 1024 {
        return Err(crate::i18n::text("backend-turn-has-too-many-files-for-review-acb5bf").into());
    }
    let raw = ctx.run(
        &[
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            parent,
            turn,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if raw.len() > TOTAL_LIMIT {
        return Err(crate::i18n::text(
            "backend-turn-diff-exceeds-32-mib-inspect-files-separately-78dc2b",
        )
        .into());
    }
    let raw = String::from_utf8(raw)
        .map_err(|_| crate::i18n::text("backend-turn-diff-cannot-be-displayed-as-utf-8-1fdb24"))?;
    let hunks = split(&raw, &known);
    Ok(Review {
        parent: parent.into(),
        turn: turn.into(),
        hunks,
        aggregate: None,
        session: session.into(),
        boundary: state.boundary,
        reference,
        old,
        known,
    })
}
pub fn mark_reviewed(
    repo: &Path,
    review: &Review,
    selected: &[usize],
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    let ctx = Context::new(repo, active)?;
    let mut known = review.known.clone();
    for &i in selected {
        known.insert(
            review
                .hunks
                .get(i)
                .ok_or(crate::i18n::text("backend-invalid-reviewed-hunk-4cc919"))?
                .digest
                .clone(),
        );
    }
    if known.len() > 10000 {
        return Err(crate::i18n::text(
            "backend-review-history-reached-10000-hunks-choose-another-session-606a19",
        )
        .into());
    }
    save_review(
        &ctx,
        review,
        ReviewState {
            schema: 1,
            known,
            boundary: review.boundary.clone(),
        },
        active,
    )
}
fn save_review(
    ctx: &Context,
    review: &Review,
    state: ReviewState,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    if review.reference != review_ref(&ctx.id, &review.session) {
        return Err(crate::i18n::text(
            "backend-review-belongs-to-a-different-worktree-session-a9bcc4",
        )
        .into());
    }
    let bytes = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::text("backend-review-state-exceeds-1-mib-226894").into());
    }
    let oid = ctx.blob(bytes, active)?;
    let zero = "0".repeat(oid.len());
    ctx.source(
        &[
            "update-ref",
            "--no-deref",
            &review.reference,
            &oid,
            if review.old.is_empty() {
                &zero
            } else {
                &review.old
            },
        ],
        vec![],
        &[0],
        active,
    )
    .map_err(|_| {
        crate::i18n::text(
            "backend-review-marks-or-boundary-changed-concurrently-reload-the-saved-re-b2c7fd",
        )
        .to_owned()
    })?;
    let mut updated = review.clone();
    updated.old = oid;
    updated.known = state.known;
    updated.boundary = state.boundary;
    for hunk in &mut updated.hunks {
        hunk.reviewed = updated.known.contains(&hunk.digest);
    }
    if let Some(aggregate) = &mut updated.aggregate {
        aggregate.last_looked = updated.boundary.clone();
    }
    Ok(updated)
}
pub fn finish_aggregate(
    repo: &Path,
    review: &Review,
    active: &dyn Fn() -> bool,
) -> Result<Review, String> {
    let state = finish_state(review, &session_turns(repo, active)?)?;
    let ctx = Context::new(repo, active)?;
    save_review(&ctx, review, state, active)
}
fn finish_state(
    review: &Review,
    turns: &[crate::agent::checkpoint::Turn],
) -> Result<ReviewState, String> {
    let aggregate = review.aggregate.as_ref().ok_or(crate::i18n::text(
        "backend-open-an-aggregate-saved-turn-review-first-940957",
    ))?;
    if aggregate.session != review.session || aggregate.last_looked != review.boundary {
        return Err(crate::i18n::text("backend-review-boundary-context-changed-a8e004").into());
    }
    let (base, _) = aggregate_span(
        turns,
        &review.session,
        &review.turn,
        review.boundary.as_deref(),
    )?;
    if base != review.parent {
        return Err(crate::i18n::text(
            "backend-aggregate-baseline-changed-reopen-before-advancing-its-boundary-533f5d",
        )
        .into());
    }
    let mut known = review.known.clone();
    known.extend(review.hunks.iter().map(|hunk| hunk.digest.clone()));
    if known.len() > 10000 {
        return Err(crate::i18n::text("backend-review-history-reached-10000-hunks-50df7f").into());
    }
    Ok(ReviewState {
        schema: 1,
        known,
        boundary: Some(review.turn.clone()),
    })
}

#[cfg(test)]
pub(crate) fn recorded_aggregate_fixture() -> Review {
    Review {
        parent: "b".repeat(40),
        turn: "c".repeat(40),
        hunks: vec![Hunk {
            patch: "new aggregate hunk".into(),
            digest: "hunk".into(),
            reviewed: false,
        }],
        aggregate: Some(Aggregate {
            session: "recorded-session".into(),
            last_looked: Some("b".repeat(40)),
            saved_turns: 1,
        }),
        session: "recorded-session".into(),
        boundary: Some("b".repeat(40)),
        reference: review_ref("recorded-worktree", "recorded-session"),
        old: "d".repeat(40),
        known: BTreeSet::new(),
    }
}

#[cfg(test)]
mod identity_tests;

#[cfg(test)]
mod aggregate_tests;

mod review_state;
#[allow(unused_imports)] // Compatibility re-export.
pub use review_state::Aggregate;
#[allow(unused_imports)] // Compatibility re-export.
pub use review_state::Hunk;
pub use review_state::Review;
use review_state::ReviewState;
pub use review_state::aggregate_review;
use review_state::aggregate_span;
#[cfg(test)]
use review_state::decode_review;
#[cfg(test)]
use review_state::digest;
use review_state::load_review;
use review_state::review_ref;
use review_state::session_turns;
use review_state::split;

mod inverse;
#[allow(unused_imports)] // Compatibility re-export.
pub use inverse::Applied;
pub use inverse::Inverse;
pub use inverse::inverse_apply;
pub use inverse::inverse_preview;
