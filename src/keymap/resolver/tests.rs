use super::*;
#[test]
fn chord_timeout_escape_mismatch_and_repeats() {
    let mut r = Resolver::default();
    r.config.insert(
        Command::Fetch.id().into(),
        vec![
            Stroke::new(Key::K, egui::Modifiers::COMMAND),
            Stroke::new(Key::F, egui::Modifiers::NONE),
        ],
    );
    let call = |r: &mut Resolver, key, mods, repeat, time| {
        r.resolve(Scope::Repository, key, None, mods, repeat, time)
    };
    assert_eq!(
        call(&mut r, Key::K, egui::Modifiers::COMMAND, false, 0.0),
        (None, true)
    );
    assert_eq!(
        call(&mut r, Key::F, egui::Modifiers::NONE, true, 0.1),
        (None, true)
    );
    assert_eq!(
        call(&mut r, Key::F, egui::Modifiers::NONE, false, 0.2),
        (Some(Command::Fetch), true)
    );
    call(&mut r, Key::K, egui::Modifiers::COMMAND, false, 1.0);
    assert_eq!(
        call(&mut r, Key::F, egui::Modifiers::NONE, false, 3.0),
        (None, false)
    );
    call(&mut r, Key::K, egui::Modifiers::COMMAND, false, 4.0);
    assert_eq!(
        call(&mut r, Key::Escape, egui::Modifiers::NONE, false, 4.1),
        (None, true)
    );
    assert!(r.prefix.is_empty());
    call(&mut r, Key::K, egui::Modifiers::COMMAND, false, 5.0);
    assert_eq!(
        call(&mut r, Key::X, egui::Modifiers::NONE, false, 5.1),
        (None, true)
    );
    assert!(r.prefix.is_empty());
}
