use super::*;
use crate::repo::review_workflow::presentation_tests::paint;

#[test]
fn fluent_review_contract_actual_observation_keeps_opaque_command_output_and_receipt()
-> Result<(), String> {
    let mut observation = Observation {
        commit: "abc".into(),
        tree: "opaque-tree".into(),
        command: "Review hunk".into(),
        code: Some(1234),
        stopped: false,
        lines: vec!["provider output {literal} --force=refs/heads/main".into()],
        machine: "ssh host{raw}".into(),
    };
    let before = serde_json::to_string(&observation).map_err(|e| e.to_string())?;
    crate::i18n::set_language("en-US");
    assert_eq!(
        observation_summary(&observation),
        "Test exit 1,234 · command Review hunk · machine ssh host{raw} · tree opaque-tree"
    );
    crate::i18n::set_language("de");
    assert_eq!(
        observation_summary(&observation),
        "Test-Exitcode 1.234 · Befehl Review hunk · Rechner ssh host{raw} · Baum opaque-tree"
    );
    assert_eq!(
        serde_json::to_string(&observation).map_err(|e| e.to_string())?,
        before
    );
    observation.stopped = true;
    assert!(observation_summary(&observation).starts_with("Test gestoppt · Befehl Review hunk"));
    observation.stopped = false;
    observation.code = None;
    assert!(observation_summary(&observation).starts_with("Test-Exitcode unbekannt"));
    let ctx = egui::Context::default();
    let output = paint(&ctx, |ui| show_observation(ui, &observation));
    assert!(
        output.contains("Beobachtete Ausgabe") && output.contains("Review hunk"),
        "{output}"
    );
    Ok(())
}
