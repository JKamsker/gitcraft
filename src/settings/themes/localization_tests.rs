use super::*;
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn actual_theme_file_paint_helpers_keep_names_and_builtin_attribution_opaque() {
    let mut info = theme::ThemeInfo {
        id: ThemeId::file("Theme 色.json"),
        name: "Theme 色".to_owned(),
        credit: "Your theme · Author {$x}".to_owned(),
        group: Group::Yours,
        palette: theme::catalog::BUILTIN[0].palette(),
    };
    let original = info.clone();
    crate::i18n::set_language("de");
    assert_eq!(theme_credit(&info), "Eigenes Farbschema · Author {$x}");
    assert_eq!(
        theme_file_error("Theme 色.json", "not readable JSON"),
        "Theme 色.json: kein lesbares JSON"
    );
    assert_eq!(info, original);
    info.group = Group::Gitcraft;
    assert_eq!(theme_credit(&info), info.credit);
    crate::i18n::set_language("en-US");
    assert_eq!(
        theme_file_error("Theme 色.json", "not readable JSON"),
        "Theme 色.json: not readable JSON"
    );
}

#[test]
fn actual_colour_swatch_translates_name_and_retains_rgb_and_selection() {
    crate::i18n::set_language("de");
    let fonts = std::cell::Cell::new(false);
    let colour = Color32::from_rgb(0x23, 0x68, 0xA2);
    let mut h = Harness::new_ui_state(
        move |ui, chosen: &mut bool| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            let t = Tokens::get(ui.ctx());
            if swatch(ui, colour, *chosen, &t).clicked() {
                *chosen = !*chosen;
            }
        },
        false,
    );
    h.run();
    h.get_by_label("Farbe #2368A2").click();
    h.run();
    assert!(*h.state());
    assert_eq!(colour.to_array(), [0x23, 0x68, 0xA2, 0xFF]);
    assert_eq!(Group::Dark.label(), "Dunkel");
    crate::i18n::set_language("en-US");
    h.step();
    h.get_by_label("Colour #2368A2");
    assert!(*h.state());
    assert_eq!(Group::Dark.label(), "Dark");
}
