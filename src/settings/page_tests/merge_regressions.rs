use super::*;
use egui_kittest::kittest::{NodeT, Queryable};

#[test]
fn compact_page_picker_repaints_both_languages_and_keeps_enum_selection() {
    let mut h = egui_kittest::Harness::new_ui_state(compact_page_picker, View::default());
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        h.run();
        let picker = h.get_by_role(egui::accesskit::Role::ComboBox);
        assert_eq!(
            picker.accesskit_node().data().value(),
            Some(super::super::labels::display(Page::General.title()))
        );
        picker.click();
        h.run();
        h.get_by_label(super::super::labels::display(Page::Appearance.title()))
            .click();
        h.run();
        assert_eq!(h.state().page, Page::Appearance);
        assert!(h.state().query.is_empty());
        // Reset only the selection for the next locale, using the same Context and salt.
        h.state_mut().page = Page::General;
    }
    crate::i18n::set_language("en-US");
}
