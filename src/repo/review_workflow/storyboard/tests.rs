use super::*;
#[test]
fn r2_pure_storyboard_requires_current_files_even_when_refreshed_head_is_unchanged() {
    assert!(current_files(2, 2, "head", "head"));
    assert!(!current_files(2, 1, "head", "head"));
    assert!(!current_files(2, 2, "new", "old"));
}
#[test]
fn cards_follow_range_diff_pairs_and_flag_changed_content() -> Result<(), String> {
    let r = git::stack::test_repo::TestRepo::new("storyboard")?;
    let base = r.tip("HEAD")?;
    let old = r.commit("a.rs", "original\n", "fix")?;
    let blob = file_blob(r.path(), &old, "a.rs")?;
    let c = Card {
        phase: "Fix".into(),
        text: "invariant".into(),
        base: base.clone(),
        head: old.clone(),
        anchor: Anchor::Code {
            commit: old,
            path: "a.rs".into(),
            line: 1,
            blob,
        },
    };
    r.git(&["commit", "--amend", "-m", "rewritten message"])?;
    let new = r.tip("HEAD")?;
    let mut f = PrFiles {
        from: base,
        to: new.clone(),
        files: vec![],
    };
    let shown = show_card(r.path(), c.clone(), &f)?;
    assert_eq!(shown.target.map(|t| t.0), Some(new));
    assert!(shown.stale.is_none());
    f.to = r.commit("a.rs", "different\n", "change")?;
    assert!(show_card(r.path(), c, &f)?.stale.is_some());
    Ok(())
}
#[test]
fn sharing_roundtrip_retains_rewritten_sources_and_never_moves_a_branch() -> Result<(), String> {
    let author = git::stack::test_repo::TestRepo::new("story-author")?;
    let server = git::stack::test_repo::TestRepo::new("story-server")?;
    let reviewer = git::stack::test_repo::TestRepo::new("story-reader")?;
    let upstream = git::stack::test_repo::TestRepo::new("story-upstream")?;
    let base = author.tip("HEAD")?;
    let pinned = author.commit("old.rs", "old invariant\n", "original fix")?;
    let board = Board {
        cards: vec![Card {
            phase: "Fix".into(),
            text: "why".into(),
            base: base.clone(),
            head: pinned.clone(),
            anchor: Anchor::Code {
                commit: pinned.clone(),
                path: "old.rs".into(),
                line: 1,
                blob: file_blob(author.path(), &pinned, "old.rs")?,
            },
        }],
    };
    author.git(&["reset", "--hard", &base])?;
    let server_head = server.tip("HEAD")?;
    let reviewer_head = reviewer.tip("HEAD")?;
    author.git(&[
        "remote",
        "add",
        "team",
        upstream.path().to_str().ok_or("path")?,
    ])?;
    author.git(&[
        "remote",
        "set-url",
        "--push",
        "team",
        server.path().to_str().ok_or("path")?,
    ])?;
    reviewer.git(&[
        "remote",
        "add",
        "team",
        server.path().to_str().ok_or("path")?,
    ])?;
    let observed = git::remotes(author.path())?
        .into_iter()
        .find(|r| r.name == "team")
        .ok_or("remote")?;
    let target = sharing::Target::capture(author.path(), &observed)?;
    publish_board(author.path(), "example#1", &board, &target, None)
        .map_err(|e| format!("first publish: {e}"))?;
    let observed = git::remotes(reviewer.path())?
        .into_iter()
        .find(|r| r.name == "team")
        .ok_or("remote")?;
    let read_target = sharing::Target::capture(reviewer.path(), &observed)?;
    let imported = fetch_board(reviewer.path(), "example#1", &read_target, None)
        .map_err(|e| format!("first fetch: {e}"))?;
    assert_eq!(imported.cards.len(), 1);
    assert_eq!(oid(reviewer.path(), &pinned)?, pinned);
    assert_eq!(
        git::run(
            Some(reviewer.path()),
            &["show", &format!("{pinned}:old.rs")]
        )?
        .trim(),
        "old invariant"
    );
    let previous = oid(server.path(), &shared_ref("example#1"))?;
    // First publication updates the live cache; this target retains its private view.
    // Report both witnesses if publication cannot refresh that cache.
    let cached = cached_ref("example#1", &target);
    let live_cache = oid(author.path(), &cached)?;
    let captured_cache = target.fetched(&cached)?;
    publish_board(author.path(), "example#1", &Board::default(), &target, None).map_err(|e| {
        format!("second publish (live cache {live_cache}, private cache {captured_cache:?}): {e}")
    })?;
    let latest = oid(server.path(), &shared_ref("example#1"))?;
    assert!(git::is_ancestor(server.path(), &previous, &latest));
    assert!(
        fetch_board(reviewer.path(), "example#1", &read_target, None)
            .map_err(|e| format!("second fetch: {e}"))?
            .cards
            .is_empty()
    );
    // Reusing the same captured target must refresh the already populated cache,
    // including a no-change fetch. Its old-OID guard must still refuse a stale writer.
    assert_eq!(
        oid(reviewer.path(), &cached_ref("example#1", &read_target))?,
        latest
    );
    assert!(
        fetch_board(reviewer.path(), "example#1", &read_target, None)?
            .cards
            .is_empty()
    );
    let cached = cached_ref("example#1", &read_target);
    let transaction = format!("start\nupdate {cached} {previous} {previous}\nprepare\ncommit\n");
    assert!(
        git::run_input(
            Some(reviewer.path()),
            &["update-ref", "--stdin"],
            &transaction
        )
        .is_err()
    );
    assert_eq!(oid(reviewer.path(), &cached)?, latest);
    assert_eq!(author.tip("HEAD")?, base);
    assert_eq!(server.tip("HEAD")?, server_head);
    assert_eq!(reviewer.tip("HEAD")?, reviewer_head);
    Ok(())
}
#[test]
fn r2_pure_storyboard_refresh_invalidates_rows_preserves_provenance_and_rejects_late_results() {
    let card = Card {
        phase: "Result".into(),
        text: "tested".into(),
        base: "A".into(),
        head: "B".into(),
        anchor: Anchor::Test {
            commit: "B".into(),
            tree: "treeB".into(),
            command: "test".into(),
            verdict: "good".into(),
        },
    };
    let mut state = Storyboard {
        imported: true,
        ..Default::default()
    };
    let shown = || {
        vec![Shown {
            card: card.clone(),
            target: None,
            stale: None,
        }]
    };
    state.accept(0, Ok(shown()));
    state.invalidate();
    assert!(state.rows.is_none());
    assert!(state.cached.is_some());
    assert!(state.imported);
    state.accept(0, Ok(shown()));
    assert!(state.rows.is_none());
    let request = state.request;
    state.accept(
        request,
        Ok(vec![Shown {
            card,
            target: None,
            stale: Some("Current tree differs".into()),
        }]),
    );
    assert!(
        state
            .rows
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|rows| rows[0].stale.is_some())
    );
    assert!(state.imported);
}
