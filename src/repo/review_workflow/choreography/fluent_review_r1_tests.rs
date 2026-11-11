use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

fn inert_stage(line: &str, i: usize) -> Stage {
    let (label, revision, generated_index) = stage_input(line, i);
    Stage {
        label,
        commit: revision.into(),
        tree: format!("tree-{i}"),
        generated_index,
    }
}

#[test]
fn fluent_review_r1_stage_provenance_preserves_explicit_collisions_and_legacy_labels()
-> Result<(), String> {
    for (locale, generated) in [("en-US", "Stage 1"), ("de", "Stufe 1")] {
        crate::i18n::set_language(locale);
        let stage = inert_stage("refs/heads/generated", 1);
        assert_eq!(stage.label, "Stage 1");
        assert_eq!(stage.commit, "refs/heads/generated");
        assert_eq!(stage.generated_index, Some(1));
        assert_eq!(display_stage(&stage), generated);
        let reopened: Stage =
            serde_json::from_str(&serde_json::to_string(&stage).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        assert_eq!(display_stage(&reopened), generated);
        for label in ["Stage1", "Stage 1", "Custom {raw}", "Stufe 1"] {
            let stage = inert_stage(&format!("{label}= refs/heads/opaque"), 1);
            assert_eq!(stage.generated_index, None);
            assert_eq!(stage.label, label);
            assert_eq!(display_stage(&stage), label);
            assert_eq!(stage.commit, "refs/heads/opaque");
            let serialized = serde_json::to_string(&stage).map_err(|e| e.to_string())?;
            assert!(!serialized.contains("generated_index"));
        }
        // Older reports cannot prove whether Stage 1 was generated or entered by a user.
        let legacy: Stage =
            serde_json::from_str(r#"{"label":"Stage 1","commit":"oid","tree":"tree"}"#)
                .map_err(|e| e.to_string())?;
        assert_eq!(legacy.generated_index, None);
        assert_eq!(display_stage(&legacy), "Stage 1");
    }
    Ok(())
}

#[test]
fn fluent_review_r1_actual_route_evidence_and_count_header_retain_locale_state()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.report = Some(Report {
        sources: vec![
            Source {
                name: "backend".into(),
                repo: PathBuf::from("/opaque/backend"),
                stages: vec![inert_stage("base-oid", 0), inert_stage("next-oid", 1)],
            },
            Source {
                name: "custom".into(),
                repo: PathBuf::from("/opaque/custom"),
                stages: vec![
                    inert_stage("Stage 1=userbase", 0),
                    inert_stage("Stage1=usernext", 1),
                ],
            },
        ],
        rules: vec![],
        command: "Stage --opaque-command".into(),
        states: vec![
            Combination {
                stages: vec![0, 0],
                allowed: true,
                observed: None,
                error: None,
            },
            Combination {
                stages: vec![1, 1],
                allowed: true,
                observed: None,
                error: None,
            },
        ],
        route: Some(vec![vec![0, 0], vec![1, 0], vec![1, 1]]),
        tested: false,
        stopped: false,
        cleanup: vec![],
    });
    let before =
        serde_json::to_string(state.report.as_ref().ok_or("report")?).map_err(|e| e.to_string())?;
    let mut acts = vec![];
    crate::i18n::set_language("en-US");
    let text = paint(&ctx, |ui| {
        show(ui, &mut state, Path::new("/opaque"), &mut acts)
    });
    assert!(text.contains("backend → Stage 1 (next-oid)"), "{text}");
    assert!(
        !text.contains("backend=Stage 0"),
        "evidence should start collapsed: {text}"
    );
    click(&ctx, "2 combinations and their evidence", |ui| {
        show(ui, &mut state, Path::new("/opaque"), &mut acts)
    })?;
    for (locale, generated) in [("en-US", "Stage"), ("de", "Stufe"), ("en-US", "Stage")] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| {
            show(ui, &mut state, Path::new("/opaque"), &mut acts)
        });
        assert!(
            text.contains(&format!("backend → {generated} 1 (next-oid)")),
            "{text}"
        );
        assert!(
            text.contains(&format!("backend={generated} 0 (base-oid)")),
            "evidence expansion lost: {text}"
        );
        assert!(
            text.contains(&format!("backend={generated} 1 (next-oid)")),
            "{text}"
        );
        assert!(
            text.contains("custom → Stage1 (usernext)")
                && text.contains("custom=Stage 1 (userbase)")
                && text.contains("custom=Stage1 (usernext)"),
            "explicit labels changed: {text}"
        );
    }
    assert_eq!(
        serde_json::to_string(state.report.as_ref().ok_or("report")?).map_err(|e| e.to_string())?,
        before
    );
    state
        .report
        .as_mut()
        .ok_or("report")?
        .states
        .push(Combination {
            stages: vec![0, 0],
            allowed: true,
            observed: None,
            error: None,
        });
    crate::i18n::set_language("de");
    let text = paint(&ctx, |ui| {
        show(ui, &mut state, Path::new("/opaque"), &mut acts)
    });
    assert!(
        text.contains("3 Kombinationen und ihre Nachweise") && text.contains("backend=Stufe 0"),
        "count change lost expansion: {text}"
    );
    assert!(acts.is_empty());
    Ok(())
}
