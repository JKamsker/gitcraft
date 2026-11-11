use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_intents_actual_form_keeps_refs_payload_and_stale_cache_generation()
-> Result<(), String> {
    let _pause = network::test_pause::set(None);
    let ctx = egui::Context::default();
    let mut state = State {
        remote: "team{opaque}".into(),
        path: "src/{raw}".into(),
        symbol: "Token".into(),
        message: "User text --argv".into(),
        minutes: 1,
        generation: 7,
        ..Default::default()
    };
    state.rows.push(Seen {
        reference: "refs/gitcraft/intents/opaque/id".into(),
        local: true,
        card: Card {
            id: "id".into(),
            author: "User{raw}".into(),
            email: "a@b".into(),
            path: "src/{raw}".into(),
            symbol: "Token".into(),
            message: "raw card".into(),
            created: now(),
            expires: now() + 60,
        },
    });
    let before = serde_json::to_string(&state.rows[0].card).map_err(|e| e.to_string())?;
    let remotes = [git::Remote {
        name: "team{opaque}".into(),
        url: "ssh://opaque/Repo A".into(),
    }];
    let mut acts = vec![];
    for (locale, heading, expiry) in [
        ("en-US", "Intent cards", "Expires in one minute"),
        ("de", "Absichtskarten", "Läuft in einer Minute ab"),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| show(ui, &mut state, &remotes, &mut acts));
        assert!(text.contains(heading) && text.contains(expiry), "{text}");
        assert!(
            text.contains("User{raw}") && text.contains("team{opaque}"),
            "{text}"
        );
    }
    assert_eq!(state.generation, 7);
    assert_eq!(
        serde_json::to_string(&state.rows[0].card).map_err(|e| e.to_string())?,
        before
    );
    click(
        &ctx,
        "Lokale Absicht anlegen (bei Bedarf veröffentlichen)",
        |ui| show(ui, &mut state, &remotes, &mut acts),
    )?;
    assert!(
        matches!(acts.as_slice(), [Action::ReviewTools(ToolsAct::Intent(Act::Post { path, symbol, message, minutes }))] if path == "src/{raw}" && symbol == "Token" && message == "User text --argv" && *minutes == 1)
    );
    let request = state.generation;
    state.selection_changed();
    assert!(!state.accept(request, Ok((Reading::default(), Some("old".into())))));
    assert!(state.cache_key.is_none() && state.rows.iter().all(|row| row.local));
    Ok(())
}
