use super::*;
fn card(id: &str) -> Card {
    Card {
        id: id.into(),
        author: "Teammate".into(),
        email: "team@example.test".into(),
        path: "src".into(),
        symbol: "token".into(),
        message: "changing lifetime".into(),
        created: 100,
        expires: 200,
    }
}
fn reference(root: &str, card: &Card) -> String {
    format!("{root}{:x}/{}", md5::compute(&card.email), card.id)
}
#[test]
fn r3_pure_intent_exact_budget_refuses_remaining_notes_without_fresh_sync() -> Result<(), String> {
    let count = IMPORT_LIMIT / CARD_LIMIT;
    let selected = format!("{CACHE_ROOT}selected/");
    // Exercise exactly full, one-more-in-source, and next-source boundary cases.
    for (local_count, remote_count, refused) in
        [(0, count, false), (0, count + 1, true), (count, 1, true)]
    {
        let mut reads = 0;
        let result = collect_selected(
            Some("selected"),
            150,
            |root| {
                let size = if root == ROOT {
                    local_count
                } else {
                    remote_count
                };
                assert!(root == ROOT || root == selected);
                Ok((0..size)
                    .map(|n| {
                        let card = card(&format!("{n:032x}"));
                        (reference(root, &card), n.to_string())
                    })
                    .collect())
            },
            |object, budget| {
                reads += 1;
                assert_eq!(budget, CARD_LIMIT);
                let n: usize = object.parse().map_err(|_| "invalid recorded object")?;
                let card = card(&format!("{n:032x}"));
                let mut text = serde_json::to_string(&card).map_err(|e| e.to_string())?;
                // Valid trailing JSON whitespace reaches exactly 8192 bytes.
                text.push_str(&" ".repeat(CARD_LIMIT - text.len()));
                assert_eq!(text.len(), CARD_LIMIT);
                Ok(text)
            },
        );
        assert_eq!(reads, count, "must never read the first omitted blob");
        if refused {
            let error = result.err().ok_or("silently accepted a partial read")?;
            assert!(error.contains("4MiB") && error.contains("more notes remain"));
            let mut state = State {
                synced: 150,
                ..Default::default()
            };
            assert!(state.accept(state.generation, Err(error)));
            assert_eq!(state.synced, 0);
            assert!(state.error.is_some());
        } else {
            let reading = result?;
            assert_eq!(reading.rows.len(), count);
            assert!(reading.quarantine.is_empty());
        }
    }
    Ok(())
}
#[test]
fn r2_pure_intent_sources_are_selected_before_parse_and_keep_same_id_separate() -> Result<(), String>
{
    let card = card("0123456789abcdef0123456789abcdef");
    let local = reference(ROOT, &card);
    let selected = reference("refs/gitcraft/intent-remotes/selected/", &card);
    let foreign = reference("refs/gitcraft/intent-remotes/foreign/", &card);
    let json = serde_json::to_string(&card).map_err(|e| e.to_string())?;
    let result = collect_selected(
        Some("selected"),
        150,
        |root| {
            assert!(!root.contains("foreign"));
            // Even a malformed foreign row returned by a recorder cannot cross the boundary.
            Ok(vec![
                (
                    if root == ROOT {
                        local.clone()
                    } else {
                        selected.clone()
                    },
                    "good".into(),
                ),
                (foreign.clone(), "foreign-malformed-bomb".into()),
            ])
        },
        |object, _| {
            assert_eq!(object, "good");
            Ok(json.clone())
        },
    )?;
    assert_eq!(result.rows.len(), 2);
    assert!(result.quarantine.is_empty());
    assert_eq!(source_roots(None)?, vec![ROOT]);
    assert!(source_roots(Some("../foreign")).is_err());
    Ok(())
}
#[test]
fn r2_pure_intent_quarantine_keeps_valid_neighbor_and_enforces_size_identity_path_expiry()
-> Result<(), String> {
    let valid = card("0123456789abcdef0123456789abcdef");
    let json = serde_json::to_string(&valid).map_err(|e| e.to_string())?;
    let result = collect_selected(
        None,
        150,
        |_| {
            Ok(vec![
                (reference(ROOT, &valid), "good".into()),
                (format!("{ROOT}bad"), "invalid".into()),
                (format!("{ROOT}huge"), "huge".into()),
            ])
        },
        |object, budget| {
            assert!(budget <= CARD_LIMIT);
            match object {
                "good" => Ok(json.clone()),
                "invalid" => Ok("{".into()),
                _ => Ok("x".repeat(CARD_LIMIT + 1)),
            }
        },
    )?;
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.quarantine.len(), 2);
    for path in [
        "../escape",
        "/root",
        "C:/drive",
        ".git/config",
        "src/../../x",
        "src\\x",
        "src\nnext",
    ] {
        let mut bad = valid.clone();
        bad.path = path.into();
        assert!(
            validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err(),
            "{path}"
        );
    }
    let mut bad = valid.clone();
    bad.expires = bad.created + 86401;
    assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
    bad = valid.clone();
    bad.message = "x".repeat(1025);
    assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
    bad = valid.clone();
    bad.id = "different".into();
    assert!(validate(&bad, &reference(ROOT, &valid), ROOT, 150).is_err());
    Ok(())
}
#[test]
fn r2_pure_intent_selection_generation_rejects_read_post_and_error_completions() {
    let mut state = State {
        cache_key: Some("old".into()),
        ..Default::default()
    };
    let old = state.generation;
    state.selection_changed();
    assert!(state.cache_key.is_none());
    assert!(!state.accept(
        old,
        Ok((
            Reading {
                rows: vec![Seen {
                    reference: "foreign".into(),
                    card: card("0123456789abcdef0123456789abcdef"),
                    local: false
                }],
                quarantine: vec![]
            },
            Some("old".into())
        ))
    ));
    assert!(state.rows.is_empty());
    assert!(!state.accept(old, Err("old read failure".into())));
    assert!(state.error.is_none());
    assert!(state.accept(state.generation, Ok((Reading::default(), None))));
    assert!(state.cache_key.is_none());
}
#[test]
fn shared_intents_do_not_move_branches_and_expiry_never_locks() -> Result<(), String> {
    let a = git::stack::test_repo::TestRepo::new("intent-a")?;
    let b = git::stack::test_repo::TestRepo::new("intent-b")?;
    let head = a.tip("HEAD")?;
    post(
        a.path(),
        "src".into(),
        "token".into(),
        "changing lifetime".into(),
        1,
    )?;
    b.git(&[
        "fetch",
        a.path().to_str().ok_or("path")?,
        "refs/gitcraft/intents/*:refs/gitcraft/intent-remotes/test/*",
    ])?;
    let notes = read_active(b.path(), Some("test"))?;
    assert_eq!(notes.len(), 1);
    assert!(applies(&notes[0].card, "src/auth.rs", now()));
    assert!(!applies(&notes[0].card, "src2/auth.rs", now()));
    assert!(!applies(
        &notes[0].card,
        "src/auth.rs",
        notes[0].card.expires
    ));
    assert!(read_active(b.path(), Some("replacement-source"))?.is_empty());
    assert_eq!(read_active(b.path(), Some("test"))?.len(), 1);
    assert_eq!(a.tip("HEAD")?, head);
    Ok(())
}
