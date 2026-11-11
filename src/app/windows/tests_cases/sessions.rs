#[test]
fn transfers_keep_one_live_box_and_preserve_selected_other_tabs() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![
        deferred("/virtual/alpha", &ctx),
        deferred("/virtual/beta", &ctx),
        deferred("/virtual/gamma", &ctx),
    ];
    app.active = 2;
    let alpha = identity(&app.tabs[0]);
    let gamma = identity(&app.tabs[2]);
    if let Tab::Repo(r) = &mut app.tabs[0] {
        r.finished.push(crate::repo::Notice::plain((
            "completion queued before detaching".into(),
            false,
        )));
    }
    app.move_to_window(0, None, Some(egui::pos2(-1000.0, 60.0)), &ctx);
    let first = *app.windows.parked.keys().next().ok_or("no window")?;
    assert_eq!(identity(&app.tabs[app.active]), gamma);
    let w = app.windows.parked.get(&first).ok_or("missing window")?;
    assert_eq!(identity(&w.tabs[0]), alpha);
    assert!(
        matches!(&w.tabs[0], Tab::Repo(r) if r.finished[0].0 == "completion queued before detaching")
    );
    app.move_to_window(0, Some(first), None, &ctx);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.windows.parked[&first].tabs.len(), 2);
    assert_eq!(app.all_repo_paths().len(), 3);
    // A stale closed-window target must not remove the source tab.
    app.move_to_window(0, Some(ViewportId::from_hash_of("gone")), None, &ctx);
    assert_eq!(identity(&app.tabs[0]), gamma);
    let mut w = app.windows.parked.remove(&first).ok_or("missing window")?;
    w.swap(&mut app);
    app.windows.parked.insert(ViewportId::ROOT, w);
    app.windows.current = first;
    app.return_window(ctx.clone());
    assert!(app.tabs.is_empty());
    let root = &app.windows.parked[&ViewportId::ROOT];
    assert_eq!(root.tabs.len(), 3);
    assert_eq!(identity(&root.tabs[1]), alpha);
    assert_eq!(
        root.tabs
            .iter()
            .filter_map(identity)
            .filter(|id| Some(*id) == alpha)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn saved_windows_do_not_duplicate_main_tabs_and_old_state_defaults_to_no_windows()
-> Result<(), String> {
    use eframe::App as _;
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![
        deferred("/virtual/alpha", &ctx),
        deferred("/virtual/beta", &ctx),
    ];
    app.move_to_window(1, None, Some(egui::pos2(-600.0, 10.0)), &ctx);
    #[derive(Default)]
    struct Storage(std::collections::HashMap<String, String>);
    impl eframe::Storage for Storage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }
    let mut storage = Storage::default();
    app.save(&mut storage);
    let saved: super::super::Saved =
        eframe::get_value(&storage, eframe::APP_KEY).ok_or("no saved state")?;
    assert_eq!(saved.open, [PathBuf::from("/virtual/alpha")]);
    assert_eq!(saved.windows.len(), 1);
    assert_eq!(saved.windows[0].open, [PathBuf::from("/virtual/beta")]);
    assert_eq!(saved.windows[0].position, None);
    let old: super::super::Saved =
        ron::from_str("(open:[],active:0,recent:[])").map_err(|e| e.to_string())?;
    assert!(old.windows.is_empty());
    Ok(())
}

#[test]
fn deferred_child_accepts_keys_and_closes_without_another_root_paint() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let owner = h.state().app.clone();
    let id = ViewportId::from_hash_of("independent-child");
    {
        let mut app = owner.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![deferred("/virtual/root-session", &ctx)];
        app.windows
            .parked
            .insert(id, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    }
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        DesktopApp::children(&owner, &h.state().cache, ui.ctx())
    });
    let cb = output
        .viewport_output
        .get(&id)
        .and_then(|v| v.viewport_ui_cb.clone())
        .ok_or("no deferred callback")?;
    assert!(output.viewport_output[&id].class == egui::ViewportClass::Deferred);
    output.drop_without_applying_deltas();
    let mut raw = egui::RawInput {
        viewport_id: id,
        ..Default::default()
    };
    raw.viewports.entry(ViewportId::ROOT).or_default().minimized = Some(true);
    raw.viewports.entry(id).or_default().focused = Some(true);
    raw.events.push(egui::Event::Key {
        key: egui::Key::T,
        physical_key: Some(egui::Key::T),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    let output = ctx.run_ui(raw, |ui| cb(ui));
    assert!(!output.shapes.is_empty());
    output.drop_without_applying_deltas();
    {
        let app = owner.lock().map_err(|e| e.to_string())?;
        assert_eq!(app.tabs.len(), 1, "root must not execute child's New Tab");
        assert_eq!(
            app.windows.parked[&id].tabs.len(),
            2,
            "child must execute New Tab while root sleeps"
        );
        assert!(
            matches!(&app.tabs[0], Tab::Repo(r) if r.path == Path::new("/virtual/root-session"))
        );
    }
    let mut raw = egui::RawInput {
        viewport_id: id,
        ..Default::default()
    };
    raw.viewports
        .entry(id)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run_ui(raw, |ui| cb(ui));
    output.drop_without_applying_deltas();
    let app = owner.lock().map_err(|e| e.to_string())?;
    assert!(!app.windows.parked.contains_key(&id));
    assert_eq!(
        app.all_repo_paths(),
        [PathBuf::from("/virtual/root-session")]
    );
    Ok(())
}

#[test]
fn hidden_logic_drains_each_windows_completions_once_without_a_ui_pass() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![deferred("/virtual/root", &ctx)];
    let id = ViewportId::from_hash_of("hidden-child");
    app.windows.parked.insert(
        id,
        Window::new(
            vec![deferred("/virtual/child", &ctx)],
            ViewportId::ROOT,
            None,
        ),
    );
    if let Tab::Repo(r) = &mut app.tabs[0] {
        r.finished.push(crate::repo::Notice::plain((
            "root completion".into(),
            false,
        )));
    }
    if let Tab::Repo(r) = &mut app.windows.parked.get_mut(&id).ok_or("no child")?.tabs[0] {
        r.finished.push(crate::repo::Notice::plain((
            "child completion".into(),
            true,
        )));
    }
    let before = app.toasts.len();
    let passes = ctx.cumulative_pass_nr();
    let focus = ctx.memory(|m| m.focused());
    let mut raw = egui::RawInput::default();
    raw.viewports.entry(ViewportId::ROOT).or_default().minimized = Some(true);
    let _ = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
    assert_eq!(app.toasts.len(), before + 2);
    let _ = ctx.run_logic(&raw, |ctx| app.tick_model(ctx, None));
    assert_eq!(app.toasts.len(), before + 2);
    assert_eq!(ctx.cumulative_pass_nr(), passes);
    assert_eq!(ctx.memory(|m| m.focused()), focus);
    assert_eq!(app.all_repo_paths().len(), 2);
    Ok(())
}
