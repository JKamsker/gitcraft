//! The connection section renders translated and edits only profile metadata.
use egui_kittest::{Harness, kittest::Queryable};

use super::{Account, CLOUD_ORIGINS, Edition};

#[test]
fn the_section_translates_and_switching_to_cloud_picks_a_region() {
    crate::i18n::set_language("de");
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, account: &mut Account| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            super::ui::account(ui, 4, account);
        },
        Account::default(),
    );
    h.run();
    h.get_by_label("Verbindung prüfen");
    h.get_by_label("Token sicher speichern");
    h.get_by_label("SonarQube Cloud").click();
    h.run();
    assert_eq!(h.state().edition, Edition::Cloud);
    assert_eq!(h.state().origin, CLOUD_ORIGINS[0]);
    h.get_by_label("Organisation");
    crate::i18n::set_language("en-US");
    h.step();
    h.get_by_label("Check connection");
    h.get_by_label("SonarQube Server").click();
    h.run();
    assert_eq!(h.state(), &Account::default());
}
