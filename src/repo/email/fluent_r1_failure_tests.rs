use super::*;
use egui_kittest::{Harness, kittest::Queryable};
#[test]
fn fluent_partition_r1_email_cached_failure_retains_retry_hint_and_opaque_multiline_details() {
    let detail = "provider;\nIf cancelled or partially completed: {$x} 日本語";
    crate::i18n::set_language("de");
    let mut state = State::default();
    state.remember_result(LocalResult::Failed {
        detail: detail.into(),
    });
    let raw = state.result.clone();
    let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        h.run();
        let shown = h.state().display_result();
        h.get_by_label(&shown);
        assert!(shown.contains(detail));
        assert!(shown.contains(if lang == "de" {
            "Prüfen Sie nach Abbruch"
        } else {
            "If cancelled or partially completed"
        }));
        assert_eq!(h.state().result, raw);
        assert!(h.state().pending.is_none());
        assert!(!h.state().trust && !h.state().confirm);
    }
    crate::i18n::set_language("en-US");
}
