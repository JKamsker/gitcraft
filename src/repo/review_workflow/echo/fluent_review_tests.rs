use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_echo_actual_rulebook_plural_and_pending_operation_keep_raw_controls()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let comment = Comment {
        url: "https://opaque.test/comment/{raw}".into(),
        body: "raw user --argv".into(),
        author: "user".into(),
    };
    let rule = Rule {
        summary: "User rule {raw}".into(),
        pattern: "secret\\s*=".into(),
        source: comment.url.clone(),
        enabled: false,
    };
    let mut state = State::default();
    state.book = Book {
        enabled: false,
        comments: vec![comment],
        rules: vec![rule],
    };
    state.pending_push = Some(Waiting {
        continuation: None,
        stamp: "opaque-stamp".into(),
        source: None,
        op: PendingOp {
            label: "Stage".into(),
            ops: vec![vec![
                "push".into(),
                "--force-with-lease=refs/heads/main:old".into(),
            ]],
            refresh: crate::repo::Refresh::All,
            kind: crate::repo::OpKind::plain(),
            quiet: false,
        },
    });
    let before = serde_json::to_string(&state.book).map_err(|e| e.to_string())?;
    let mut acts = vec![];
    for (locale, count, pending) in [
        (
            "en-US",
            "One cited comment · one rule.",
            "Pending operation: Stage",
        ),
        (
            "de",
            "Ein zitierter Kommentar · eine Regel.",
            "Ausstehende Operation: Vormerken",
        ),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| show(ui, &mut state, &mut acts));
        assert!(text.contains(count) && text.contains(pending), "{text}");
        assert!(text.contains("User rule {raw}"), "{text}");
    }
    assert_eq!(
        serde_json::to_string(&state.book).map_err(|e| e.to_string())?,
        before
    );
    assert_eq!(
        state.pending_push.as_ref().ok_or("pending")?.op.label,
        "Stage"
    );
    click(
        &ctx,
        "Ich habe diese Beobachtungen geprüft; Operation fortsetzen",
        |ui| show(ui, &mut state, &mut acts),
    )?;
    assert!(
        matches!(acts.as_slice(), [Action::ReviewTools(ToolsAct::Echo(Act::PushAnyway(stamp)))] if stamp == "opaque-stamp")
    );
    assert_eq!(
        state.pending_push.as_ref().ok_or("pending")?.op.ops[0][1],
        "--force-with-lease=refs/heads/main:old"
    );
    Ok(())
}
