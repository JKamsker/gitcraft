use super::*;
fn turns() -> Vec<crate::agent::checkpoint::Turn> {
    (1..=3)
        .map(|n| crate::agent::checkpoint::Turn {
            session: "recorded-session".into(),
            number: n,
            commit: char::from(b'a' + n as u8).to_string().repeat(40),
            parent: char::from(b'a' + n as u8 - 1).to_string().repeat(40),
            ..Default::default()
        })
        .collect()
}
#[test]
fn pure_aggregate_review_fixed_endpoint_excludes_new_turns_and_refuses_changed_baselines()
-> Result<(), String> {
    let mut review = recorded_aggregate_fixture();
    let turns = turns();
    assert_eq!(
        aggregate_span(
            &turns,
            "recorded-session",
            &review.turn,
            review.boundary.as_deref()
        )
        .map_err(|error| error.to_string())?,
        ("b".repeat(40), 1)
    );
    let mut advanced = review.clone();
    advanced.old = "e".repeat(40);
    assert!(!review.same_snapshot(&advanced));
    advanced = review.clone();
    advanced.reference = review_ref("other-worktree", "recorded-session");
    assert!(!review.same_snapshot(&advanced));
    assert!(review.same_snapshot(&review.clone()));
    let state = finish_state(&review, &turns).map_err(|error| error.to_string())?;
    assert_eq!(state.boundary.as_deref(), Some(review.turn.as_str()));
    assert_ne!(state.boundary.as_deref(), Some(turns[2].commit.as_str()));
    assert!(state.known.contains("hunk"));
    review.parent = "a".repeat(40);
    assert!(finish_state(&review, &turns).is_err());
    review = recorded_aggregate_fixture();
    review
        .aggregate
        .as_mut()
        .ok_or("expected Some")?
        .last_looked = Some("c".repeat(40));
    assert!(finish_state(&review, &turns).is_err());
    let mut changed = turns.clone();
    changed[1].parent = "f".repeat(40);
    assert!(finish_state(&recorded_aggregate_fixture(), &changed).is_err());
    assert!(aggregate_span(&turns, "other-session", &review.turn, None).is_err());
    assert_ne!(
        review_ref("worktree-a", "session"),
        review_ref("worktree-b", "session")
    );
    assert_ne!(
        review_ref("worktree-a", "session"),
        review_ref("worktree-a", "other-session")
    );
    Ok(())
}
#[test]
fn pure_aggregate_review_migrates_legacy_marks_without_inventing_a_review_boundary()
-> Result<(), String> {
    let old: BTreeSet<String> = ["old-hunk-key".into()].into();
    let state = decode_review(&serde_json::to_vec(&old).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    assert_eq!(state.known, old);
    assert!(state.boundary.is_none());
    let current =
        finish_state(&recorded_aggregate_fixture(), &turns()).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_vec(&current).map_err(|error| error.to_string())?;
    assert_eq!(
        decode_review(&encoded)
            .map_err(|error| error.to_string())?
            .boundary,
        current.boundary
    );
    assert!(decode_review(br#"{"schema":2,"known":[],"boundary":null}"#).is_err());
    Ok(())
}
