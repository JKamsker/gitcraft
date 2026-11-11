use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_notice_toast_repaints_retained_source_and_preserves_raw_lifetime() {
    let notice = crate::repo::Notice::authorship(
        "topic: $(keep)".into(),
        crate::authorship::branch::Figures::default(),
    );
    let raw = notice.raw().clone();
    let toast = Toast::from_notice(notice);
    let until = toast.until;
    crate::i18n::set_language("en-US");
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, toast: &mut Toast| {
            if !fonts.replace(true) {
                theme::install_fonts(ui.ctx());
                return;
            }
            toast.label(ui);
        },
        toast,
    );
    h.run();
    h.get_by_label("topic: $(keep) adds no lines since it left the base");
    crate::i18n::set_language("de");
    h.step();
    h.get_by_label("topic: $(keep) fügt seit der Abzweigung von der Basis keine Zeilen hinzu.");
    assert_eq!((&h.state().text, h.state().error), (&raw.0, raw.1));
    assert_eq!(h.state().until, until);
    assert!(h.state().update_go_back.is_none());
    crate::i18n::set_language("en-US");
    h.step();
    h.get_by_label(&raw.0);
    assert_eq!(h.state().notice.as_ref().map(|n| n.raw()), Some(&raw));
    // A normal toast with identical text has no authorship receipt.
    let plain = Toast::new(raw.0.clone(), raw.1);
    crate::i18n::set_language("de");
    assert_eq!(plain.display_text(), raw.0);
    let toast = h.state_mut();
    toast.text = "unrelated provider error".into();
    assert_eq!(toast.display_text(), "unrelated provider error");
}
