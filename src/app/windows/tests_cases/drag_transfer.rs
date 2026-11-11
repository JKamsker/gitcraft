#[test]
fn captured_release_after_source_exit_detaches_the_live_tab() -> Result<(), String> {
    use egui::{Event, Modifiers, PointerButton};
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![
        deferred("/virtual/dragged", &ctx),
        deferred("/virtual/stays", &ctx),
    ];
    let owned = identity(&app.tabs[0]);
    let mut time = 1.0;
    let mut frame = |app: &mut App, events| {
        time += 0.1;
        let mut raw = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        raw.viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .inner_rect = Some(Rect::from_min_size(
            egui::pos2(100.0, 200.0),
            egui::vec2(1200.0, 800.0),
        ));
        ctx.run_ui(raw, |ui| {
            ui.set_min_size(egui::vec2(1200.0, 800.0));
            app.tab_strip(ui);
        })
        .drop_without_applying_deltas();
    };
    frame(&mut app, vec![]);
    let press = egui::pos2(40.0, 20.0);
    frame(
        &mut app,
        vec![
            Event::PointerMoved(press),
            Event::PointerButton {
                pos: press,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    let outside = egui::pos2(160.0, 140.0);
    frame(&mut app, vec![Event::PointerMoved(outside)]);
    assert!(app.tab_drag.is_some());
    frame(&mut app, vec![Event::PointerGone]);
    for _ in 0..3 {
        frame(&mut app, vec![]);
        assert!(
            app.tab_drag.is_some(),
            "leaving the source must retain the captured drag"
        );
        assert_eq!(identity(&app.tabs[0]), owned);
    }
    // Recorded backend output: CursorMoved at the native captured mouse-up location
    // MUST precede MouseInput Released, even though no ordinary motion arrived after exit.
    let released = egui::pos2(-240.0, 140.0);
    frame(
        &mut app,
        vec![
            Event::PointerMoved(released),
            Event::PointerButton {
                pos: released,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert!(app.tab_drag.is_none());
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.windows.parked.len(), 1);
    let window = app
        .windows
        .parked
        .values()
        .next()
        .ok_or("drag did not detach")?;
    assert_eq!(identity(&window.tabs[0]), owned);
    assert_eq!(window.place_at_pixels, Some(egui::pos2(-140.0, 340.0)));
    Ok(())
}

#[test]
fn captured_release_after_source_exit_targets_an_existing_window() -> Result<(), String> {
    use egui::{Event, Modifiers, PointerButton};
    let h = harness();
    let ctx = h.ctx.clone();
    ctx.set_embed_viewports(false);
    let mut app = h.state().app.lock().map_err(|e| e.to_string())?;
    app.tabs = vec![
        deferred("/virtual/dragged", &ctx),
        deferred("/virtual/stays", &ctx),
    ];
    let peer = ViewportId::from_hash_of("existing-drop-target");
    app.windows.parked.insert(
        peer,
        Window::new(
            vec![deferred("/virtual/peer", &ctx)],
            ViewportId::ROOT,
            None,
        ),
    );
    app.windows.strips.insert(
        peer,
        Rect::from_min_size(egui::pos2(-200.0, 320.0), egui::vec2(100.0, 40.0)),
    );
    let owned = identity(&app.tabs[0]);
    let mut time = 1.0;
    let mut frame = |app: &mut App, events| {
        time += 0.1;
        let mut raw = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        raw.viewports
            .entry(ViewportId::ROOT)
            .or_default()
            .inner_rect = Some(Rect::from_min_size(
            egui::pos2(100.0, 200.0),
            egui::vec2(1200.0, 800.0),
        ));
        ctx.run_ui(raw, |ui| {
            ui.set_min_size(egui::vec2(1200.0, 800.0));
            app.tab_strip(ui);
        })
        .drop_without_applying_deltas();
    };
    frame(&mut app, vec![]);
    let press = egui::pos2(40.0, 20.0);
    frame(
        &mut app,
        vec![
            Event::PointerMoved(press),
            Event::PointerButton {
                pos: press,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    let outside = egui::pos2(160.0, 140.0);
    frame(&mut app, vec![Event::PointerMoved(outside)]);
    assert!(app.tab_drag.is_some());
    frame(&mut app, vec![Event::PointerGone]);
    for _ in 0..3 {
        frame(&mut app, vec![]);
        assert!(
            app.tab_drag.is_some(),
            "leaving the source must retain the captured drag"
        );
        assert_eq!(identity(&app.tabs[0]), owned);
    }
    // Recorded backend output: CursorMoved at the native captured mouse-up location
    // MUST precede MouseInput Released, even though no ordinary motion arrived after exit.
    let released = egui::pos2(-240.0, 140.0);
    frame(
        &mut app,
        vec![
            Event::PointerMoved(released),
            Event::PointerButton {
                pos: released,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert!(app.tab_drag.is_none());
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.windows.parked.len(), 1);
    let window = app
        .windows
        .parked
        .values()
        .next()
        .ok_or("drag did not detach")?;
    assert!(app.windows.parked.contains_key(&peer));
    assert_eq!(window.tabs.len(), 2);
    assert_eq!(identity(&window.tabs[window.active]), owned);
    assert_eq!(app.all_repo_paths().len(), 3);
    Ok(())
}
