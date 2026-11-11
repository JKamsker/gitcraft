use super::*;
use crate::repo::review_workflow::presentation_tests::{click, paint};

#[test]
fn fluent_review_choreography_actual_board_repaints_numeric_plurals_and_keeps_plan()
-> Result<(), String> {
    let ctx = egui::Context::default();
    let mut state = State::default();
    state.plan.components.push(Component {
        name: "backend{literal}".into(),
        repo: "/opaque/Repo A".into(),
        revisions: "deployed=refs/heads/main".into(),
    });
    state.report = Some(Report {
        sources: vec![],
        rules: vec![],
        command: "printf 'keep argv'".into(),
        states: vec![Combination {
            stages: vec![],
            allowed: true,
            observed: None,
            error: None,
        }],
        route: None,
        tested: false,
        stopped: false,
        cleanup: vec![],
    });
    let before = serde_json::to_string(&state.plan).map_err(|e| e.to_string())?;
    let mut acts = vec![];
    for (locale, heading, count) in [
        (
            "en-US",
            "Release choreography across repositories",
            "One combination and its evidence",
        ),
        (
            "de",
            "Release-Choreografie über mehrere Repositorys",
            "Eine Kombination und ihre Nachweise",
        ),
    ] {
        crate::i18n::set_language(locale);
        let text = paint(&ctx, |ui| {
            show(ui, &mut state, Path::new("/opaque/Repo A"), &mut acts)
        });
        assert!(text.contains(heading) && text.contains(count), "{text}");
        assert!(
            text.contains("backend{literal}") && text.contains("deployed=refs/heads/main"),
            "{text}"
        );
    }
    state
        .report
        .as_mut()
        .ok_or("report")?
        .states
        .push(Combination {
            stages: vec![],
            allowed: true,
            observed: None,
            error: None,
        });
    let text = paint(&ctx, |ui| {
        show(ui, &mut state, Path::new("/opaque/Repo A"), &mut acts)
    });
    assert!(
        text.contains("2 Kombinationen und ihre Nachweise"),
        "{text}"
    );
    assert_eq!(
        serde_json::to_string(&state.plan).map_err(|e| e.to_string())?,
        before
    );
    click(&ctx, "Board speichern", |ui| {
        show(ui, &mut state, Path::new("/opaque/Repo A"), &mut acts)
    })?;
    assert!(
        matches!(acts.as_slice(), [Action::ReviewTools(ToolsAct::Choreography(Act::Save(plan)))] if plan.components[0].repo == "/opaque/Repo A")
    );
    Ok(())
}
