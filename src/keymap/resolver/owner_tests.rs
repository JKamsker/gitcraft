use super::*;
#[test]
fn modal_restriction_leaves_unrelated_editor_keys_unconsumed() {
    let ctx = Context::default();
    restrict(&ctx, Some(vec![Command::Palette, Command::Manual]));
    let key = Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    };
    ctx.run_ui(
        egui::RawInput {
            events: vec![key.clone()],
            ..Default::default()
        },
        |_| {
            assert!(pressed(&ctx, Scope::Window).is_empty());
            assert!(pressed(&ctx, Scope::Repository).is_empty());
            assert!(
                ctx.input(|i| i.events.contains(&key)),
                "terminal/editor Ctrl+Enter stays available"
            );
        },
    )
    .textures_delta
    .clear();
}
