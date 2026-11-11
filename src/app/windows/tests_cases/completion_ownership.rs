#[test]
fn practice_modal_and_completion_belong_to_the_initiating_window() -> Result<(), String> {
    let h = recorded_harness();
    let ctx = h.ctx.clone();
    let mut app = h
        .state()
        .as_ref()
        .ok_or("App")?
        .app
        .lock()
        .map_err(|e| e.to_string())?;
    let child = ViewportId::from_hash_of("practice-owner");
    let path = PathBuf::from("/virtual/practice-repo");
    app.tabs = vec![Tab::Home];
    app.windows.parked.insert(
        child,
        Window::new(
            vec![deferred(path.to_str().ok_or("path")?, &ctx), Tab::Home],
            ViewportId::ROOT,
            None,
        ),
    );
    app.in_window(child, |app| {
        app.active = 1;
        app.practice_open = true;
        assert!(app.window_has_dialog());
    });
    assert!(!app.practice_open);
    assert!(!app.window_has_dialog());
    app.recorded_practice_completion(child, Ok(path));
    app.poll_practice(&ctx);
    assert_eq!(app.windows.parked[&child].active, 0);
    assert!(!app.windows.parked[&child].practice_open);
    assert!(matches!(app.tabs[0], Tab::Home));
    app.recorded_practice_completion(child, Err("recorded practice failure".into()));
    app.poll_practice(&ctx);
    assert!(app.error.is_none());
    assert_eq!(
        app.windows.parked[&child].error.as_deref(),
        Some("recorded practice failure")
    );
    Ok(())
}

#[test]
fn inbox_open_waits_for_repository_parked_in_another_window() -> Result<(), String> {
    let h = recorded_harness();
    let ctx = h.ctx.clone();
    let mut app = h
        .state()
        .as_ref()
        .ok_or("App")?
        .app
        .lock()
        .map_err(|e| e.to_string())?;
    let child = ViewportId::from_hash_of("inbox-owner");
    let path = PathBuf::from("/virtual/inbox-repo");
    app.tabs = vec![Tab::Home];
    app.windows.parked.insert(
        child,
        Window::new(
            vec![deferred(path.to_str().ok_or("path")?, &ctx)],
            ViewportId::ROOT,
            None,
        ),
    );
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        assert!(app.recorded_inbox_open(path, ctx));
    });
    assert!(
        output
            .platform_output
            .commands
            .iter()
            .all(|command| !matches!(command, egui::OutputCommand::OpenUrl(_)))
    );
    app.poll_inbox_opening(&ctx);
    assert!(matches!(&app.windows.parked[&child].tabs[0], Tab::Repo(_)));
    assert!(matches!(app.tabs[0], Tab::Home));
    Ok(())
}

#[test]
fn closed_owner_falls_back_to_root_not_the_polling_peer() -> Result<(), String> {
    let h = harness();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    let peer = ViewportId::from_hash_of("polling-peer");
    app.windows
        .parked
        .insert(peer, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    app.in_window(peer, |app| {
        app.in_window(ViewportId::from_hash_of("closed-owner"), |app| {
            assert_eq!(app.windows.current, ViewportId::ROOT);
            app.error = Some("recorded result".to_owned().into());
        });
        assert_eq!(app.windows.current, peer);
        assert!(app.error.is_none());
    });
    assert_eq!(app.error.as_deref(), Some("recorded result"));
    Ok(())
}

#[test]
fn hidden_logic_advances_the_patch_queue_without_a_paint_pass() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![Tab::Home];
    app.patches
        .push("recorded.patch".into(), "/no-live-session".into());
    let before = app.toasts.len();
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.tick_model(ctx, None));
    assert!(app.patches.is_empty());
    assert_eq!(app.toasts.len(), before + 1);
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.tick_model(ctx, None));
    assert_eq!(app.toasts.len(), before + 1);
    Ok(())
}
