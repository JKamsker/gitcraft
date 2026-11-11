use super::*;
fn frame(ctx: &Context, time: f64, mut events: Vec<Event>) -> (Vec<Command>, Vec<Command>) {
    let mut result = (Vec::new(), Vec::new());
    let releases: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Key {
                key,
                physical_key,
                modifiers,
                ..
            } => Some(Event::Key {
                key: *key,
                physical_key: *physical_key,
                modifiers: *modifiers,
                pressed: false,
                repeat: false,
            }),
            _ => None,
        })
        .collect();
    events.extend(releases);
    let input = egui::RawInput {
        time: Some(time),
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |_| {
        result.0 = pressed(ctx, Scope::Window);
        result.1 = pressed(ctx, Scope::Repository);
        assert!(
            pressed(ctx, Scope::Repository).is_empty(),
            "each event is delivered once"
        );
    })
    .textures_delta
    .clear();
    result
}
fn key(key: Key, modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        modifiers,
        pressed: true,
        repeat: false,
    }
}
#[test]
fn shared_prefix_across_scopes_delivers_to_correct_owner_and_displays_override() {
    let ctx = Context::default();
    let mut settings = crate::settings::Settings::default();
    settings.key_bindings.insert(
        Command::NewTab.id().into(),
        vec![
            Stroke::new(Key::K, egui::Modifiers::COMMAND),
            Stroke::new(Key::T, egui::Modifiers::NONE),
        ],
    );
    settings.key_bindings.insert(
        Command::Fetch.id().into(),
        vec![
            Stroke::new(Key::K, egui::Modifiers::COMMAND),
            Stroke::new(Key::F, egui::Modifiers::NONE),
        ],
    );
    assert!(config::validate(&settings.key_bindings).is_ok());
    settings.put_for_test(&ctx);
    assert!(super::super::label(&ctx, Command::Fetch).ends_with(" → F"));
    assert_eq!(
        frame(&ctx, 0.0, vec![key(Key::K, egui::Modifiers::COMMAND)]),
        (vec![], vec![])
    );
    assert_eq!(
        frame(&ctx, 0.1, vec![key(Key::F, egui::Modifiers::NONE)]),
        (vec![], vec![Command::Fetch])
    );
    frame(&ctx, 0.2, vec![key(Key::K, egui::Modifiers::COMMAND)]);
    assert_eq!(
        frame(&ctx, 0.3, vec![key(Key::T, egui::Modifiers::NONE)]),
        (vec![Command::NewTab], vec![])
    );
    frame(&ctx, 0.4, vec![key(Key::K, egui::Modifiers::COMMAND)]);
    owner(&ctx, Id::new("other-tab"));
    assert_eq!(
        frame(&ctx, 0.5, vec![key(Key::F, egui::Modifiers::NONE)]),
        (vec![], vec![])
    );
    frame(&ctx, 0.6, vec![key(Key::K, egui::Modifiers::COMMAND)]);
    ctx.memory_mut(|m| m.request_focus(Id::new("text-editor")));
    assert_eq!(
        frame(&ctx, 0.7, vec![key(Key::F, egui::Modifiers::NONE)]),
        (vec![], vec![])
    );
}
