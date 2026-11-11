#[test]
fn contended_logic_defers_close_then_replays_keep_children_or_explicit_quit() -> Result<(), String>
{
    for quit in [false, true] {
        let mut h = harness();
        let ctx = h.ctx.clone();
        let owner = h.state().app.clone();
        let child = ViewportId::from_hash_of("kept-during-contention");
        let mut held = owner.lock().map_err(|e| e.to_string())?;
        held.windows
            .parked
            .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
        held.windows.quit_requested = quit;
        let mut raw = egui::RawInput::default();
        let info = raw.viewports.entry(ViewportId::ROOT).or_default();
        info.minimized = Some(true);
        info.events.push(egui::ViewportEvent::Close);
        let output = ctx.run_logic(&raw, |ctx| h.state_mut().logic_tick(ctx, None));
        assert!(
            output.viewport_commands[&ViewportId::ROOT]
                .contains(&egui::ViewportCommand::CancelClose)
        );
        assert!(h.state().deferred_close);
        drop(held);
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            h.state_mut().logic_tick(ctx, None)
        });
        let commands = &output.viewport_commands[&ViewportId::ROOT];
        if quit {
            assert!(commands.contains(&egui::ViewportCommand::Close));
            assert!(!commands.contains(&egui::ViewportCommand::CancelClose));
        } else {
            assert!(commands.contains(&egui::ViewportCommand::CancelClose));
            assert!(commands.contains(&egui::ViewportCommand::Visible(false)));
        }
        assert!(!h.state().deferred_close);
        assert!(
            owner
                .lock()
                .map_err(|e| e.to_string())?
                .windows
                .parked
                .contains_key(&child)
        );
    }
    Ok(())
}

#[test]
fn contended_root_ui_uses_the_same_deferred_close_policy() -> Result<(), String> {
    let mut h = harness();
    let ctx = h.ctx.clone();
    let owner = h.state().app.clone();
    let child = ViewportId::from_hash_of("kept-during-ui-contention");
    let mut held = owner.lock().map_err(|e| e.to_string())?;
    held.windows
        .parked
        .insert(child, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    let mut raw = egui::RawInput::default();
    raw.viewports
        .entry(ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_ui(raw, |ui| h.state_mut().ui_tick(ui, None));
    assert!(
        output.viewport_output[&ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose)
    );
    assert!(h.state().deferred_close);
    output.drop_without_applying_deltas();
    drop(held);
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        h.state_mut().logic_tick(ctx, None)
    });
    assert!(
        output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::CancelClose)
    );
    assert!(
        owner
            .lock()
            .map_err(|e| e.to_string())?
            .windows
            .parked
            .contains_key(&child)
    );
    assert!(!h.state().deferred_close);
    Ok(())
}
