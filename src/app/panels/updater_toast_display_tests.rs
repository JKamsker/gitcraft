use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn retained_go_back_toast_paints_en_de_en_with_unchanged_raw_flag_and_deadline() {
    let failure = update::Failure::NoPrevious {
        raw: "No previous version of gitcraft is kept".into(),
    };
    let toast = Toast::from_go_back_failure(failure.clone());
    let raw = toast.text.clone();
    let until = toast.until;
    assert_eq!(
        raw,
        "Couldn't go back: No previous version of gitcraft is kept"
    );
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
    h.get_by_label(&raw);
    crate::i18n::set_language("de");
    h.step();
    h.get_by_label(
        "Zurückwechseln fehlgeschlagen: Es wurde keine vorherige Version von gitcraft aufbewahrt",
    );
    assert_eq!(h.state().text, raw);
    assert!(h.state().error);
    assert_eq!(h.state().until, until);
    assert_eq!(
        h.state().update_go_back.as_ref().map(|s| &s.failure),
        Some(&failure)
    );
    crate::i18n::set_language("en-US");
    h.step();
    h.get_by_label(&raw);
    assert_eq!(h.state().text, raw);
    assert_eq!(h.state().until, until);
}

#[test]
fn go_back_receipt_is_opt_in_and_never_inferred_from_equal_text_or_child_spelling() {
    crate::i18n::set_language("de");
    let raw = "No previous version of gitcraft is kept";
    let opaque = Toast::from_go_back_failure(update::Failure::Opaque(raw.into()));
    assert_eq!(
        opaque.display_text(),
        format!("Zurückwechseln fehlgeschlagen: {raw}")
    );
    let ordinary = Toast::new(opaque.text.clone(), true);
    assert!(ordinary.update_go_back.is_none());
    assert_eq!(ordinary.display_text(), ordinary.text);
    let ordinary_secs = Toast::for_secs("unrelated".into(), false, 10);
    assert!(ordinary_secs.update_go_back.is_none());
    let mut mismatched =
        Toast::from_go_back_failure(update::Failure::NoPrevious { raw: raw.into() });
    mismatched.text = "Couldn't go back: unrelated raw".into();
    assert_eq!(mismatched.display_text(), mismatched.text);
    mismatched.text = format!("Couldn't go back: {raw}");
    mismatched.error = false;
    assert_eq!(mismatched.display_text(), mismatched.text);
}
