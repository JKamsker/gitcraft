use super::*;
#[test]
fn fluent_partition_r1_actual_maintenance_selection_retains_raw_control_and_cached_command_provenance()
-> Result<(), String> {
    let suggestions = health::suggestions(&Health::default());
    let suggestion = suggestions
        .iter()
        .find(|s| s.title == "Scheduled maintenance")
        .ok_or("missing test value")?;
    let commands: Vec<_> = suggestion
        .ops
        .iter()
        .map(|op| format!("git {}", op.join(" ")))
        .collect();
    crate::i18n::set_language("de");
    let Action::Doctor(DoctorAct::Confirm(confirm)) = turn_on_action(suggestion, &commands) else {
        return Err("maintenance must confirm".into());
    };
    let raw = (
        confirm.title.clone(),
        confirm.body.clone(),
        confirm.button.clone(),
        confirm.label.clone(),
        confirm.ops.clone(),
    );
    assert_eq!(raw.0, "Schedule maintenance?");
    assert_eq!(raw.3, "Turn on: scheduled maintenance");
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        let (title, body, button) = confirm.displayed();
        assert_eq!(title == raw.0, lang == "en-US");
        assert_eq!(button == raw.2, lang == "en-US");
        assert!(body.starts_with(&commands.join(", ")));
        assert_eq!(
            (
                confirm.title.clone(),
                confirm.body.clone(),
                confirm.button.clone(),
                confirm.label.clone(),
                confirm.ops.clone()
            ),
            raw
        );
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
