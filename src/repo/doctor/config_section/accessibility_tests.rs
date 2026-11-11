use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
#[test]
fn config_disclosure_exposes_the_setting_and_keyboard_expansion() {
    let setting = &SETTINGS[0];
    let label = format!("{}: Git default. {}", setting.key, setting.about_label());
    let label = crate::a11y::labels::name(&label);
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut ConfigState| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            setting_row(
                ui,
                state,
                setting,
                &[],
                &Env::new(0, false, false),
                &mut Vec::new(),
            );
            crate::a11y::end_frame(ui.ctx());
        },
        ConfigState::default(),
    );
    h.run();
    assert_eq!(
        h.get_by_label(&label).accesskit_node().data().is_expanded(),
        Some(false)
    );
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(h.get_by_label(&label).is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().open, Some(setting.key));
    assert_eq!(
        h.get_by_label(&label).accesskit_node().data().is_expanded(),
        Some(true)
    );
}
