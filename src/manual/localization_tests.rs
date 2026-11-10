use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_manual_tables_and_all_tips_follow_language_with_unchanged_main_keys() {
    let ctx = egui::Context::default();
    crate::i18n::set_language("en-US");
    let english: Vec<_> = shortcut_groups(&ctx)
        .into_iter()
        .take(3)
        .flat_map(|(_, rows)| rows.into_iter().map(|(key, _)| key))
        .collect();
    assert_eq!(
        tips()[0],
        "Double-click a branch pill to check that branch out."
    );
    crate::i18n::set_language("de");
    let groups = shortcut_groups(&ctx);
    let german_keys: Vec<_> = groups
        .iter()
        .take(3)
        .flat_map(|(_, rows)| rows.iter().map(|(key, _)| key.clone()))
        .collect();
    assert_eq!(german_keys, english);
    assert_eq!(groups[0].0, "Fenster");
    assert_eq!(groups[0].1[0].1, "Neuer Tab");
    assert_eq!(tips().len(), 21);
    for tip in tips() {
        assert!(!tip.starts_with("interaction-"));
        assert!(tip.chars().count() <= 95, "tip no longer fits: {tip}");
    }
    assert!(tips()[10].contains("Tags"));
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui(|ui| {
        if !fonts.replace(true) {
            crate::theme::install_fonts(ui.ctx());
            return;
        }
        shortcut_ui(ui);
    });
    h.run();
    h.get_by_label("TASTENKÜRZEL FÜR FENSTER");
    h.get_by_label("Neuer Tab");
    crate::i18n::set_language("en-US");
    assert_eq!(
        tips()[0],
        "Double-click a branch pill to check that branch out."
    );
}
