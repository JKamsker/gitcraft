#[test]
fn reentrant_native_callbacks_keep_window_registrations_without_locking_again() -> Result<(), String>
{
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let owner = h.state().app.clone();
    let cache = h.state().cache.clone();
    let id = ViewportId::from_hash_of("reentrant-child");
    let panel = crate::repo::popout::viewport_id(
        Path::new("/virtual/panel-owner"),
        crate::repo::popout::Panel::Diff,
    );
    {
        let mut app = owner.lock().map_err(|e| e.to_string())?;
        app.tabs = vec![deferred("/virtual/panel-owner", &ctx)];
        if let Tab::Repo(repo) = &mut app.tabs[0] {
            repo.set_test_popout(crate::repo::popout::Panel::Diff);
        }
    }
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = seen.clone();
    egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
        recorded.borrow_mut().push(viewport.ids.this);
        let id = viewport.ids.this;
        let mut raw = egui::RawInput {
            viewport_id: id,
            ..Default::default()
        };
        raw.viewports.entry(id).or_default();
        ctx.run_ui(raw, |ui| (viewport.viewport_ui_cb)(ui))
            .drop_without_applying_deltas();
    });
    owner
        .lock()
        .map_err(|e| e.to_string())?
        .windows
        .parked
        .insert(id, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.ctx()
            .show_viewport_immediate(panel, ViewportBuilder::default(), |_, _| {});
        DesktopApp::children(&owner, &cache, ui.ctx())
    });
    let cb = out
        .viewport_output
        .get(&id)
        .and_then(|v| v.viewport_ui_cb.clone())
        .ok_or("no callback")?;
    out.drop_without_applying_deltas();
    let app = owner.lock().map_err(|e| e.to_string())?;
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        DesktopApp::children(&owner, &cache, ui.ctx())
    });
    assert!(
        out.viewport_output.contains_key(&id),
        "a nested native event loop must not prune the live child"
    );
    assert!(
        out.viewport_output.contains_key(&panel),
        "the immediate panel must survive the same reentrant pass"
    );
    assert_eq!(seen.borrow().iter().filter(|id| **id == panel).count(), 2);
    out.drop_without_applying_deltas();
    let mut raw = egui::RawInput {
        viewport_id: id,
        ..Default::default()
    };
    raw.viewports.entry(id).or_default();
    let out = ctx.run_ui(raw, |ui| cb(ui));
    out.drop_without_applying_deltas();
    assert_eq!(app.windows.parked[&id].tabs.len(), 1);
    Ok(())
}

#[test]
fn restore_deduplicates_sessions_and_preserves_selection_after_missing_paths() -> Result<(), String>
{
    let h = harness();
    let ctx = h.ctx.clone();
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![deferred("ssh://fixture/root", &ctx)];
    app.restore_windows(
        vec![
            SavedWindow {
                open: vec![
                    "/gitcraft/nonexistent-window-path".into(),
                    "ssh://fixture/first".into(),
                    "ssh://fixture/second".into(),
                ],
                active: 1,
                position: Some([1_000_000.0, -1_000_000.0]),
                size: Some([1.0, 0.0]),
            },
            SavedWindow {
                open: vec![
                    "ssh://fixture/first".into(),
                    "ssh://fixture/first".into(),
                    "ssh://fixture/root".into(),
                ],
                ..Default::default()
            },
        ],
        &ctx,
    );
    assert_eq!(app.windows.parked.len(), 1);
    assert_eq!(app.all_repo_paths().len(), 3);
    let window = app
        .windows
        .parked
        .values()
        .next()
        .ok_or("no restored window")?;
    assert_eq!(
        repo_path(&window.tabs[window.active]),
        Some(PathBuf::from("ssh://fixture/first"))
    );
    assert_eq!(window.position, None);
    assert_eq!(window.size, None);
    Ok(())
}

#[test]
fn desktop_drop_only_targets_another_windows_strip() {
    let a = ViewportId::ROOT;
    let b = ViewportId::from_hash_of("second");
    let strips = BTreeMap::from([
        (
            a,
            Rect::from_min_size(egui::pos2(-800.0, 10.0), egui::vec2(800.0, 40.0)),
        ),
        (
            b,
            Rect::from_min_size(egui::pos2(40.0, 100.0), egui::vec2(900.0, 40.0)),
        ),
    ]);
    assert_eq!(drop_target(a, egui::pos2(100.0, 120.0), &strips), Some(b));
    assert_eq!(drop_target(a, egui::pos2(100.0, 300.0), &strips), None);
    assert_eq!(drop_target(a, egui::pos2(-100.0, 20.0), &strips), None);
}
