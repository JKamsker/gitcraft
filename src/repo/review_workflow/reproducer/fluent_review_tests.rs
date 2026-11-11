use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_reproducer_actual_form_has_no_execution_or_approval_side_effects()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.request = 13;
    state.case = Case {
        base: "refs/heads/base".into(),
        head: "refs/heads/head".into(),
        path: "src/{opaque}.rs".into(),
        line: "1234".into(),
        objection: "User objection {raw}".into(),
        source: "#[test] fn opaque() { assert!(true); }".into(),
        image: "rust:1.95.0".into(),
        approved: false,
    };
    let before = state.case.clone();
    let mut acts = vec![];
    for (locale, heading, action) in [
        (
            "en-US",
            "AI objection → concrete reproducer",
            "Run identical test on base and head in sandbox",
        ),
        (
            "de",
            "KI-Einwand → konkreter Reproduktionsfall",
            "Identischen Test auf Basis und Head in der Sandbox ausführen",
        ),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| show(ui, &mut state, &mut acts));
        assert!(text.contains(heading) && text.contains(action), "{text}");
        assert!(
            text.contains("refs/heads/head") && text.contains("src/{opaque}.rs"),
            "{text}"
        );
    }
    assert!(state.case == before && !state.busy && !state.case.approved);
    assert_eq!(state.request, 13);
    assert!(acts.is_empty());
    click(
        &ctx,
        "Identischen Test auf Basis und Head in der Sandbox ausführen",
        |ui| show(ui, &mut state, &mut acts),
    )?;
    assert!(
        acts.is_empty(),
        "unapproved sandbox button must stay disabled"
    );
    click(
        &ctx,
        "Zeig es mir: Test mit ausgewählter KI generieren",
        |ui| show(ui, &mut state, &mut acts),
    )?;
    assert!(
        matches!(acts.as_slice(), [Action::ReviewTools(ToolsAct::Reproducer(Act::Generate(case)))] if case == &before)
    );
    Ok(())
}
