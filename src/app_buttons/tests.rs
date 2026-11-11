use super::*;

use AppButton::{Manual, Palette, Settings as Gear, Theme};

fn zone(place: Place, flow: Flow, slots: &[(f32, f32)]) -> Zone {
    Zone {
        place,
        catch: Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 400.0)),
        flow,
        slots: slots.iter().map(|&(x, y)| pos2(x, y)).collect(),
    }
}

#[test]
fn localized_actual_menu_uses_effective_chord_and_keeps_button_event_identity() -> Result<(), String>
{
    use crate::keymap::{self, Command, Stroke};
    use egui_kittest::{Harness, kittest::Queryable};
    crate::i18n::set_language("de");
    let mut settings = Settings {
        language: "de".into(),
        ..Settings::default()
    };
    settings.key_bindings.insert(
        Command::Palette.id().into(),
        vec![
            Stroke::new(egui::Key::F8, egui::Modifiers::NONE),
            Stroke::new(egui::Key::F9, egui::Modifiers::NONE),
        ],
    );
    settings
        .key_bindings
        .insert(Command::Manual.id().into(), vec![]);
    let fonts = std::cell::Cell::new(false);
    let mut h = Harness::new_ui_state(
        move |ui, clicked: &mut bool| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                egui_extras::install_image_loaders(ui.ctx());
                return;
            }
            settings.set(ui.ctx());
            assert_eq!(Palette.tooltip(ui.ctx()), "Befehlspalette (F8 → F9)");
            assert_eq!(Manual.tooltip(ui.ctx()), "Handbuch und Tastenkürzel");
            assert_eq!(keymap::label(ui.ctx(), Command::Palette), "F8 → F9");
            menu_entries(ui, &[Palette, Manual]);
            *clicked |= take_events(ui.ctx())
                .iter()
                .any(|event| matches!(event, Event::Clicked(AppButton::Palette)));
        },
        false,
    );
    h.run();
    h.get_by_label("Befehlspalette F8 → F9").click();
    h.run();
    assert!(*h.state());
    assert_eq!(Palette.command(), Some(Command::Palette));
    assert_eq!(
        serde_json::to_string(&Place::BottomLeft).map_err(|error| format!("{error:?}"))?,
        "\"BottomLeft\""
    );
    crate::i18n::set_language("en-US");

    Ok(())
}

#[test]
fn every_button_starts_in_the_title_bar() {
    let layout = ButtonLayout::default();
    assert_eq!(layout.title_bar, AppButton::ALL);
    assert!(layout.bottom_left.is_empty() && layout.bottom_right.is_empty());
}

#[test]
fn a_button_moves_to_another_place_at_an_index() {
    let layout = ButtonLayout::default().moved(Theme, Place::BottomRight, 0);
    let layout = layout.moved(Manual, Place::BottomRight, 0);
    assert_eq!(layout.bottom_right, [Manual, Theme]);
    assert_eq!(layout.title_bar, [Gear, Palette]);
}

#[test]
fn a_button_is_reordered_within_its_place() {
    let layout = ButtonLayout::default().moved(Gear, Place::TitleBar, 99);
    assert_eq!(layout.title_bar, [Palette, Theme, Manual, Gear]);
    let layout = layout.moved(Manual, Place::TitleBar, 0);
    assert_eq!(layout.title_bar, [Manual, Palette, Theme, Gear]);
}

#[test]
fn a_saved_layout_is_made_whole() {
    let saved = ButtonLayout {
        title_bar: vec![Theme, Theme],
        bottom_left: vec![Theme, Manual],
        bottom_right: Vec::new(),
    };
    let layout = saved.normalized();
    assert_eq!(layout.title_bar, [Theme, Gear, Palette]);
    assert_eq!(layout.bottom_left, [Manual]);
}

#[test]
fn older_settings_without_a_layout_get_the_default() {
    let settings: Settings = ron::from_str("(ui_scale: 1.25)").unwrap_or_default();
    assert_eq!(settings.button_layout, ButtonLayout::default());
    let saved = "(title_bar: [Settings], bottom_left: [Theme], bottom_right: [Manual, Palette])";
    let layout: ButtonLayout = ron::from_str(saved).unwrap_or_default();
    assert_eq!(layout.bottom_right, [Manual, Palette]);
    assert_eq!(layout.normalized(), layout);
}

#[test]
fn the_drop_is_before_the_first_button_not_passed() {
    // A title bar's buttons from the right edge leftwards.
    let zones = [zone(
        Place::TitleBar,
        Flow::RightToLeft,
        &[(300.0, 20.0), (266.0, 20.0)],
    )];
    let at = |x| drop_at(&zones, pos2(x, 20.0)).map(|d| d.index);
    assert_eq!(at(320.0), Some(0));
    assert_eq!(at(280.0), Some(1));
    assert_eq!(at(200.0), Some(2));
}

#[test]
fn a_column_drops_from_the_bottom_up() {
    let zones = [zone(
        Place::BottomLeft,
        Flow::BottomUp,
        &[(18.0, 380.0), (18.0, 350.0)],
    )];
    let at = |y| drop_at(&zones, pos2(18.0, y)).map(|d| d.index);
    assert_eq!(at(395.0), Some(0));
    assert_eq!(at(360.0), Some(1));
    assert_eq!(at(300.0), Some(2));
}

#[test]
fn the_nearer_place_wants_the_button_however_far_both_are() {
    let mut top = zone(Place::TitleBar, Flow::RightToLeft, &[]);
    top.catch = Rect::from_min_max(pos2(900.0, 0.0), pos2(1000.0, 40.0));
    let mut bottom = zone(Place::BottomRight, Flow::RightToLeft, &[]);
    bottom.catch = Rect::from_min_max(pos2(900.0, 800.0), pos2(1000.0, 840.0));
    let place = |y| drop_at(&[top.clone(), bottom.clone()], pos2(500.0, y)).map(|d| d.place);
    assert_eq!(place(410.0), Some(Place::TitleBar));
    assert_eq!(place(430.0), Some(Place::BottomRight));
    assert!(drop_at(&[], pos2(0.0, 0.0)).is_none());
}
