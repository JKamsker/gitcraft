use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_storyboard_actual_phase_adapter_keeps_persisted_metadata_and_stale_generation()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let details = PrDetails {
        author: "author".into(),
        head_oid: "head{opaque}".into(),
        ..Default::default()
    };
    let files = PrFiles {
        from: "base{opaque}".into(),
        to: details.head_oid.clone(),
        files: vec![],
    };
    let card = Card {
        phase: "Old failure".into(),
        text: "User story --argv {raw}".into(),
        base: files.from.clone(),
        head: files.to.clone(),
        anchor: Anchor::Test {
            commit: "opaque".into(),
            tree: "tree{raw}".into(),
            command: "cargo test --offline --test user_test".into(),
            verdict: "bad".into(),
        },
    };
    let before = serde_json::to_string(&card).map_err(|e| e.to_string())?;
    let mut state = Storyboard {
        request: 41,
        rows: Some(Ok(vec![Shown {
            card,
            target: None,
            stale: None,
        }])),
        imported: true,
        ..Default::default()
    };
    let mut acts = vec![];
    for (locale, heading, phase, verdict) in [
        ("en-US", "Review storyboard", "Old failure", "Observed bad"),
        (
            "de",
            "Review-Storyboard",
            "Früherer Fehler",
            "Beobachtet schlecht",
        ),
    ] {
        crate::i18n::set_language(locale);
        if locale == "en-US" {
            click(&ctx, heading, |ui| {
                show(ui, &mut state, &details, &files, &[], &mut acts)
            })?;
        }
        let text = paint(&ctx, |ui| {
            show(ui, &mut state, &details, &files, &[], &mut acts)
        });
        assert!(text.contains(phase) && text.contains(verdict), "{text}");
        assert!(
            text.contains("cargo test --offline --test user_test")
                && text.contains("User story --argv {raw}"),
            "{text}"
        );
    }
    assert_eq!(
        display_phase("Unknown user phase {raw}"),
        "Unknown user phase {raw}"
    );
    assert_eq!(display_verdict("custom --verdict"), "custom --verdict");
    let stored = &state
        .rows
        .as_ref()
        .ok_or("rows")?
        .as_ref()
        .map_err(|e| e.as_str())?[0]
        .card;
    assert_eq!(
        serde_json::to_string(stored).map_err(|e| e.to_string())?,
        before
    );
    assert_eq!(state.request, 41);
    assert!(acts.is_empty());
    state.accept(40, Err("stale response".into()));
    assert!(state.rows.as_ref().ok_or("rows")?.is_ok());
    Ok(())
}
