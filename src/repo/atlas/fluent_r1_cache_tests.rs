use super::*;
#[test]
fn fluent_partition_r1_atlas_cached_validation_keeps_inputs_and_page() -> Result<(), String> {
    crate::i18n::set_language("de");
    let mut state = State {
        from: "raw bad date".into(),
        through: "2026-01-01".into(),
        error: Some(invalid_range_error()),
        ..Default::default()
    };
    let raw = state.error.clone().ok_or("missing test value")?;
    state.page = 7;
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        assert_eq!(
            crate::repo::activity::display_workflow_error(&raw) == raw,
            lang == "en-US"
        );
        assert_eq!(state.error.as_ref(), Some(&raw));
        assert_eq!(state.from, "raw bad date");
        assert_eq!(state.page, 7);
        assert!(!state.reader.busy());
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
