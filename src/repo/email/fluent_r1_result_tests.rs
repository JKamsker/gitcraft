use super::*;
use egui_kittest::{Harness, kittest::Queryable};
#[test]
fn fluent_partition_r1_email_same_retained_state_repaints_without_work_or_consent_change() {
    for note in [
        LocalResult::Prepared,
        LocalResult::Received,
        LocalResult::Preview,
        LocalResult::ProfileChanged,
        LocalResult::WorkerEnded,
        LocalResult::RecipientsChanged,
        LocalResult::Running,
    ] {
        crate::i18n::set_language("de");
        let mut state = State {
            trust: true,
            ..Default::default()
        };
        state.remember_result(note);
        let raw = state.result.clone();
        let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            h.run();
            h.get_by_label(&h.state().display_result());
            assert_eq!(h.state().display_result() == raw, lang == "en-US");
            assert_eq!(h.state().result, raw);
            assert!(h.state().trust);
            assert!(h.state().pending.is_none());
        }
        h.state_mut().result_presentation = None;
        for lang in ["de", "en-US"] {
            crate::i18n::set_language(lang);
            assert_eq!(h.state().display_result(), raw);
        }
    }
    crate::i18n::set_language("en-US");
}
