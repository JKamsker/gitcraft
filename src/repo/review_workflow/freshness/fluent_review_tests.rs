use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_freshness_actual_receipt_repaints_without_recomputing_generations()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let files = PrFiles {
        from: "base{opaque}".into(),
        to: "head{opaque}".into(),
        files: vec![],
    };
    let mut state = Freshness {
        request: 9,
        rows: Some(Ok(vec![Row {
            path: "src/a{raw}.rs".into(),
            status: Status::Unchanged,
            reviewed: Some((files.from.clone(), files.to.clone())),
            dependencies_checked: true,
            rust: Some(rust_dependencies::Report {
                checked: 1,
                ..Default::default()
            }),
        }])),
        dependencies: "src/dependency{raw}.rs".into(),
    };
    let mut acts = vec![];
    for (locale, label, plural) in [
        (
            "en-US",
            "Review freshness",
            "One unchanged Rust symbol inspected",
        ),
        (
            "de",
            "Aktualität des Reviews",
            "Ein unverändertes Rust-Symbol untersucht",
        ),
    ] {
        crate::i18n::set_language(locale);
        // Expand the actual localized header (same stable ID in both languages).
        if locale == "en-US" {
            click(&ctx, label, |ui| show(ui, &mut state, &files, &mut acts))?;
        }
        let text = paint(&ctx, |ui| show(ui, &mut state, &files, &mut acts));
        assert!(
            text.contains(plural) && text.contains("src/a{raw}.rs"),
            "{text}"
        );
    }
    assert_eq!(state.request, 9);
    click(&ctx, "Datei als geprüft markieren", |ui| {
        show(ui, &mut state, &files, &mut acts)
    })?;
    assert!(
        matches!(acts.as_slice(), [Action::Freshness(FreshnessAct::Mark { path, base, head, dependencies })] if path == "src/a{raw}.rs" && base == &files.from && head == &files.to && dependencies == &["src/dependency{raw}.rs"])
    );
    Ok(())
}
