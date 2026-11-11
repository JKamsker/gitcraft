#[test]
fn restored_bounds_delegate_placement_and_fit_the_available_monitor() {
    assert_eq!(
        bounded_size(egui::vec2(5000.0, 4000.0), Some(egui::vec2(1280.0, 720.0))),
        egui::vec2(1216.0, 656.0)
    );
    assert_eq!(
        bounded_size(egui::vec2(f32::NAN, 0.0), Some(egui::vec2(640.0, 480.0))),
        egui::vec2(576.0, 416.0)
    );
}

#[test]
fn mixed_dpi_strips_and_source_pointer_use_physical_desktop_pixels() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![deferred("/virtual/drag-source", &ctx)];
    let peer = ViewportId::from_hash_of("two-times-scale");
    app.windows
        .parked
        .insert(peer, Window::new(vec![Tab::Home], ViewportId::ROOT, None));
    let mut raw = egui::RawInput {
        viewport_id: peer,
        ..Default::default()
    };
    let info = raw.viewports.entry(peer).or_default();
    info.native_pixels_per_point = Some(2.0);
    info.inner_rect = Some(Rect::from_min_size(
        egui::pos2(800.0, 50.0),
        egui::vec2(400.0, 300.0),
    ));
    app.windows.current = peer;
    ctx.run_ui(raw, |ui| {
        app.window_strip(
            ui.ctx(),
            Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 20.0)),
        )
    })
    .drop_without_applying_deltas();
    assert_eq!(
        app.windows.strips[&peer],
        Rect::from_min_size(egui::pos2(1600.0, 100.0), egui::vec2(800.0, 40.0))
    );
    app.windows.current = ViewportId::ROOT;
    let mut raw = egui::RawInput {
        events: vec![egui::Event::PointerMoved(egui::pos2(1700.0, 120.0))],
        ..Default::default()
    };
    let info = raw.viewports.entry(ViewportId::ROOT).or_default();
    info.native_pixels_per_point = Some(1.0);
    info.inner_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0)));
    ctx.run_ui(raw, |ui| {
        assert_eq!(
            app.drop_window_target(
                0,
                Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 40.0)),
                ui.ctx()
            ),
            Some((Some(peer), egui::pos2(1700.0, 120.0)))
        );
    })
    .drop_without_applying_deltas();
    Ok(())
}

#[test]
fn new_window_applies_physical_drop_position_using_its_actual_scale() -> Result<(), String> {
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let owner = h.state().app.clone();
    let id = ViewportId::from_hash_of("positioned-child");
    let physical = egui::pos2(1700.0, 120.0);
    let mut window = Window::new(vec![Tab::Home], ViewportId::ROOT, None);
    window.place_at_pixels = Some(physical);
    owner
        .lock()
        .map_err(|e| e.to_string())?
        .windows
        .parked
        .insert(id, window);
    let mut raw = egui::RawInput {
        viewport_id: id,
        ..Default::default()
    };
    raw.viewports.entry(id).or_default().native_pixels_per_point = Some(2.0);
    let output = ctx.run_ui(raw, |ui| {
        DesktopApp::child(&Arc::downgrade(&owner), &h.state().cache, id, ui)
    });
    assert!(
        output.viewport_output[&id]
            .commands
            .contains(&egui::ViewportCommand::OuterPosition(physical / 2.0))
    );
    output.drop_without_applying_deltas();
    assert!(
        owner.lock().map_err(|e| e.to_string())?.windows.parked[&id]
            .place_at_pixels
            .is_none()
    );
    Ok(())
}
