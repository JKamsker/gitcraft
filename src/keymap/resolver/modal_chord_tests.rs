use super::*;

fn event(key: Key, modifiers: egui::Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn permitted_modal_chords_survive_repository_blocking_and_do_not_type_continuations() {
    for (end, expected) in [(Key::P, vec![Command::Palette]), (Key::X, vec![])] {
        let ctx = Context::default();
        let mut settings = crate::settings::Settings::default();
        settings.key_bindings.insert(
            Command::Palette.id().into(),
            vec![
                Stroke::new(Key::K, egui::Modifiers::COMMAND),
                Stroke::new(Key::P, egui::Modifiers::NONE),
            ],
        );
        settings.put_for_test(&ctx);
        let mut text = String::new();
        for (pass, events) in [
            vec![],
            vec![event(Key::K, egui::Modifiers::COMMAND)],
            vec![
                event(end, egui::Modifiers::NONE),
                Event::Text(if end == Key::P { "p" } else { "x" }.into()),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(pass as f64 * 0.1),
                    events,
                    ..Default::default()
                },
                |ui| {
                    restrict(&ctx, Some(vec![Command::Palette, Command::Manual]));
                    let commands = pressed(&ctx, Scope::Window);
                    if pass == 2 {
                        assert_eq!(commands, expected);
                    } else {
                        assert!(commands.is_empty());
                    }
                    retain_allowed(&ctx);
                    let r = ui.text_edit_singleline(&mut text);
                    if pass == 0 {
                        r.request_focus();
                    }
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(
            text, "",
            "completed and rejected chord strokes belong to the resolver"
        );
    }
}

#[test]
fn blocked_repository_cancels_prefix_without_a_deliberate_exception() {
    let ctx = Context::default();
    let key = id(&ctx, Scope::Window);
    let mut r = Resolver::default();
    r.prefix.push(Stroke::new(Key::K, egui::Modifiers::COMMAND));
    ctx.data_mut(|d| d.insert_temp(key, r));
    retain_allowed(&ctx);
    assert!(
        ctx.data(|d| d.get_temp::<Resolver>(key))
            .is_some_and(|r| r.prefix.is_empty())
    );
}

#[test]
fn overlay_and_dispatch_use_the_same_active_and_allowed_candidates() {
    let r = Resolver {
        active: vec![Scope::Window, Scope::Repository, Scope::Files],
        allowed: Some(vec![Command::Palette, Command::HunkStage]),
        ..Default::default()
    };
    assert!(r.eligible(Command::Palette));
    assert!(
        !r.eligible(Command::FileNext),
        "modal exclusions apply to which-key too"
    );
    assert!(
        !r.eligible(Command::HunkStage),
        "an inactive diff binding is not a continuation"
    );
}
