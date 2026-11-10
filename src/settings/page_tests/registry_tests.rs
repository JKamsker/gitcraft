use super::*;
use crate::keymap::{Command, Stroke as KeyStroke};
#[test]
fn configured_page_and_search_keys_use_actual_settings_handler() {
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    settings.key_bindings.insert(
        Command::SettingsNext.id().into(),
        vec![KeyStroke::new(Key::F8, egui::Modifiers::NONE)],
    );
    settings.key_bindings.insert(
        Command::SettingsSearch.id().into(),
        vec![KeyStroke::new(Key::F9, egui::Modifiers::NONE)],
    );
    settings.put_for_test(&ctx);
    let mut view = View::default();
    let before = view.page;
    for key in [Key::F8, Key::F9] {
        ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                keys(ui, &mut view, false);
                ui.add(egui::TextEdit::singleline(&mut view.query).id(search_id()));
            },
        )
        .drop_without_applying_deltas();
    }
    assert_ne!(view.page, before);
    assert!(ctx.memory(|memory| memory.has_focus(search_id())));
}
