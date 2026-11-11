#[test]
fn resident_missing_git_still_accepts_quit_handoff_and_keeps_git_actions_guarded()
-> Result<(), String> {
    let h = recorded_harness();
    let ctx = h.ctx.clone();
    let mut app = h
        .state()
        .as_ref()
        .ok_or("no recorded App")?
        .app
        .lock()
        .map_err(|e| e.to_string())?;
    app.git_version = Err("Git is unavailable (recorded)".into());
    app.settings.keep_running_in_tray = true;
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_handoff(
            crate::instance::Request::Launch {
                cwd: Default::default(),
                args: vec!["--clone".into()],
            },
            ctx,
        );
    });
    assert!(app.clone_form.is_none());
    assert!(!app.windows.quit_requested);
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        assert!(app.root_close_request(ctx, true));
    });
    assert!(app.windows.root_hidden);
    assert!(
        output.viewport_commands[&ViewportId::ROOT]
            .contains(&egui::ViewportCommand::Visible(false))
    );
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_handoff(
            crate::instance::Request::Launch {
                cwd: Default::default(),
                args: vec!["--quit".into()],
            },
            ctx,
        );
        assert!(!app.root_close_request(ctx, true));
    });
    assert!(app.windows.quit_requested);
    assert!(output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::Close));
    assert!(
        !output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::CancelClose)
    );
    assert!(
        !output.viewport_commands[&ViewportId::ROOT]
            .contains(&egui::ViewportCommand::Visible(true))
    );
    Ok(())
}

#[test]
fn resident_close_hides_only_when_opted_in_and_quit_bypasses_hide() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    assert!(!app.settings.keep_running_in_tray);
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        assert!(!app.root_close_request(ctx, true));
    });
    assert!(
        !output
            .viewport_commands
            .get(&ViewportId::ROOT)
            .is_some_and(|commands| commands.contains(&egui::ViewportCommand::CancelClose))
    );
    app.settings.keep_running_in_tray = true;
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        assert!(app.root_close_request(ctx, true));
    });
    assert!(app.windows.root_hidden);
    assert!(
        output.viewport_commands[&ViewportId::ROOT]
            .contains(&egui::ViewportCommand::Visible(false))
    );
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_handoff(crate::instance::Request::Focus, ctx)
    });
    assert!(!app.windows.root_hidden);
    assert!(
        output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::Visible(true))
    );
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.launch(crate::launch::Command::Quit, Default::default(), ctx);
        assert!(!app.root_close_request(ctx, true));
    });
    assert!(app.windows.quit_requested);
    assert!(output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::Close));
    assert!(
        !output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::CancelClose)
    );
    Ok(())
}

#[test]
fn hidden_root_close_is_cancelled_in_logic_but_explicit_quit_is_not() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.windows.parked.insert(
        ViewportId::from_hash_of("child"),
        Window::new(vec![Tab::Home], ViewportId::ROOT, None),
    );
    let mut raw = egui::RawInput::default();
    let info = raw.viewports.entry(ViewportId::ROOT).or_default();
    info.minimized = Some(true);
    info.events.push(egui::ViewportEvent::Close);
    let passes = ctx.cumulative_pass_nr();
    let output = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
    assert!(
        output.viewport_commands[&ViewportId::ROOT].contains(&egui::ViewportCommand::CancelClose)
    );
    assert!(
        output.viewport_commands[&ViewportId::ROOT]
            .contains(&egui::ViewportCommand::Visible(false))
    );
    assert_eq!(ctx.cumulative_pass_nr(), passes);
    app.windows.quit_requested = true;
    let output = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
    assert!(
        !output
            .viewport_commands
            .get(&ViewportId::ROOT)
            .is_some_and(|commands| commands.contains(&egui::ViewportCommand::CancelClose))
    );
    Ok(())
}
