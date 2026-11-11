use super::*;
#[test]
#[ignore = "R2 runtime hold: requires Docker and an explicitly preloaded Rust image; authored, not executed"]
fn r2_runtime_same_integration_test_observes_base_failure_head_success_without_host_checkout()
-> Result<(), String> {
    let image = std::env::var("GITCRAFT_REPRODUCER_TEST_IMAGE")
        .map_err(|_| "Set GITCRAFT_REPRODUCER_TEST_IMAGE to an explicitly preloaded Rust image")?;
    let r = git::stack::test_repo::TestRepo::new("reproducer-contained-observations")?;
    r.commit(
        "Cargo.toml",
        r#"[package]
name="fixture"
version="0.1.0"
edition="2021"
"#,
        "manifest",
    )?;
    let base = r.commit(
        "src/lib.rs",
        "pub fn behavior() -> bool { false }\n",
        "base",
    )?;
    let head = r.commit("src/lib.rs", "pub fn behavior() -> bool { true }\n", "head")?;
    let original = std::fs::read(r.path().join("src/lib.rs")).map_err(|e| e.to_string())?;
    let report = run(
        r.path(),
        Case {
            base,
            head: head.clone(),
            path: "src/lib.rs".into(),
            source: "#[test] fn concrete() { assert!(fixture::behavior()); }".into(),
            image,
            approved: true,
            ..Default::default()
        },
        &crate::tasks::Cancellation::default(),
    )?;
    assert_eq!(report.base.code, Some(101));
    assert_eq!(report.head.code, Some(0));
    assert!(report.cleanup.is_empty(), "{:?}", report.cleanup);
    assert_eq!(r.tip("HEAD")?, head);
    assert_eq!(
        std::fs::read(r.path().join("src/lib.rs")).map_err(|e| e.to_string())?,
        original
    );
    Ok(())
}
#[test]
fn r2_pure_delayed_ai_completion_preserves_edited_form_and_cancelled_approval() {
    let original = Case {
        objection: "old objection".into(),
        source: "old test".into(),
        ..Default::default()
    };
    let generated = Case {
        source: "model test".into(),
        approved: true,
        ..original.clone()
    };
    let mut state = State {
        case: original.clone(),
        request: 1,
        busy: true,
        generated: None,
        cancel: Default::default(),
        stop: Default::default(),
        error: None,
        report: None,
    };
    state.case.objection = "edited objection".into();
    state.case.source = "handwritten test".into();
    state.accept(1, &original, Ok((Some(generated.clone()), None)));
    assert_eq!(state.case.source, "handwritten test");
    assert_eq!(state.case.objection, "edited objection");
    assert!(
        state
            .generated
            .as_ref()
            .is_some_and(|result| !result.approved)
    );
    state.request = 2;
    state.case.approved = false;
    state.accept(1, &original, Ok((Some(generated.clone()), None)));
    assert_eq!(state.case.source, "handwritten test");
    state.cancel.cancel();
    state.accept(2, &original, Ok((Some(generated), None)));
    assert_eq!(state.case.source, "handwritten test");
    assert!(!state.case.approved);
}
