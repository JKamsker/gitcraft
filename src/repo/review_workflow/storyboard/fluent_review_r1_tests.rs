use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_r1_actual_storyboard_commands_are_opaque_even_when_operation_names_collide()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let details = PrDetails {
        author: "author".into(),
        head_oid: "head".into(),
        ..Default::default()
    };
    let files = PrFiles {
        from: "base".into(),
        to: "head".into(),
        files: vec![],
    };
    let mut state = Storyboard {
        rows: Some(Ok(vec![Shown {
            card: Card {
                phase: "Result".into(),
                text: "User story {raw}".into(),
                base: "base".into(),
                head: "head".into(),
                anchor: Anchor::Test {
                    commit: "oid".into(),
                    tree: "tree{opaque}".into(),
                    command: "Stage".into(),
                    verdict: "bad".into(),
                },
            },
            target: None,
            stale: None,
        }])),
        imported: true,
        ..Default::default()
    };
    let mut acts = vec![];
    crate::i18n::set_language("en-US");
    click(&ctx, "Review storyboard", |ui| {
        show(ui, &mut state, &details, &files, &[], &mut acts)
    })?;
    const PREFIX: &str = "Configured when pinned (execution command not recorded by test lane): ";
    let commands = [
        "Stage".to_owned(),
        "Undo Stage".to_owned(),
        "Preparing Stage".to_owned(),
        "cargo test --offline --test user_test".to_owned(),
        "Custom {raw} --argv=Stage".to_owned(),
        format!("{PREFIX}Stage"),
        format!("{PREFIX}Undo Stage --argv={{raw}}"),
        format!(" {PREFIX}Stage"),
        "Configured when pinned: Stage".to_owned(),
    ];
    for (locale, envelope) in [
        ("en-US", PREFIX),
        (
            "de",
            "Beim Fixieren konfiguriert (Ausführungsbefehl nicht von der Testspur aufgezeichnet): ",
        ),
    ] {
        crate::i18n::set_language(locale);
        for command in &commands {
            let rows = state
                .rows
                .as_mut()
                .ok_or("rows")?
                .as_mut()
                .map_err(|e| e.to_string())?;
            let Anchor::Test {
                command: recorded, ..
            } = &mut rows[0].card.anchor
            else {
                return Err("anchor".into());
            };
            *recorded = command.clone();
            let before = serde_json::to_string(&rows[0].card).map_err(|e| e.to_string())?;
            let expected = command
                .strip_prefix(PREFIX)
                .map_or_else(|| command.clone(), |suffix| format!("{envelope}{suffix}"));
            let text = paint(&ctx, |ui| {
                show(ui, &mut state, &details, &files, &[], &mut acts)
            });
            assert!(
                text.contains(&format!("tree{{opaque}} · {expected}")),
                "recorded command reinterpreted in {locale}: {text}"
            );
            assert!(
                !text.contains("Vormerken"),
                "operation translation reached command data: {text}"
            );
            let rows = state
                .rows
                .as_ref()
                .ok_or("rows")?
                .as_ref()
                .map_err(|e| e.to_string())?;
            assert_eq!(
                serde_json::to_string(&rows[0].card).map_err(|e| e.to_string())?,
                before
            );
        }
    }
    assert!(acts.is_empty());
    Ok(())
}
