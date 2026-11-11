use super::*;
use crate::repo::review_workflow::presentation_tests::paint;

#[test]
fn fluent_review_radar_actual_coverage_plural_preserves_patch_evidence_and_argv()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.rows = Some(Ok(vec![Row {
        name: "fix{raw}".into(),
        fix: "opaque-fix".into(),
        target: "refs/heads/release".into(),
        tip: "1234567890abcdef".into(),
        patch: Patch::Same("opaque-commit".into()),
        test: None,
        coverage: 1,
        complete: false,
        error: None,
    }]));
    let mut acts = vec![];
    for (locale, coverage, patch) in [
        (
            "en-US",
            "One nonmerge commit inspected",
            "Same patch found: opaque-commit",
        ),
        (
            "de",
            "Ein Commit ohne Merge untersucht",
            "Identischen Patch gefunden: opaque-commit",
        ),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| show(ui, &mut state, &mut acts));
        assert!(text.contains(coverage) && text.contains(patch), "{text}");
        assert!(
            text.contains("refs/heads/release") && text.contains("fix{raw}"),
            "{text}"
        );
    }
    assert!(acts.is_empty());
    assert!(
        matches!(state.rows.as_ref().ok_or("rows")?.as_ref().map_err(|e| e.as_str())?[0].patch, Patch::Same(ref oid) if oid == "opaque-commit")
    );
    Ok(())
}
