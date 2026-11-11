use super::*;
#[test]
fn fluent_partition_r1_test_lane_cached_read_error_preserves_raw_detail_and_generation()
-> Result<(), String> {
    let detail = "provider; ;\nHistory worker stopped: {$x} 日本語";
    crate::i18n::set_language("de");
    let mut state = TestLane::default();
    state.remember_read_error(detail.into());
    let raw = state.error.clone().ok_or("missing test value")?;
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        let shown = state.display_read_error(&raw);
        assert!(shown.contains(detail));
        assert_eq!(shown == raw, lang == "en-US");
        assert_eq!(state.error.as_ref(), Some(&raw));
    }
    state.read_error = None;
    crate::i18n::set_language("de");
    assert_eq!(state.display_read_error(&raw), raw);
    crate::i18n::set_language("en-US");

    Ok(())
}
