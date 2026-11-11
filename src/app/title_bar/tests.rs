use super::*;

/// Exercise the actual hitboxes and viewport output without opening a native window.
fn title_bar_clicks(maximized: bool, caption: bool) -> Vec<egui::ViewportCommand> {
    let ctx = egui::Context::default();
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(CAPTIONS_W, 28.0));
    let point = rect.center();
    let frame = |time, events| {
        let mut input = egui::RawInput {
            screen_rect: Some(window()),
            time: Some(time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .maximized = Some(maximized);
        let mut output = ctx.run_ui(input, |ui| {
            if caption {
                captions(ui, &Tokens::default(), rect);
            } else {
                drag_area(ui, rect, "test");
            }
        });
        let commands = output
            .viewport_output
            .remove(&egui::ViewportId::ROOT)
            .map(|viewport| viewport.commands)
            .unwrap_or_default();
        output.drop_without_applying_deltas();
        commands
    };
    let click = |pressed| egui::Event::PointerButton {
        pos: point,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(1.0, vec![egui::Event::PointerMoved(point)]);
    let first = frame(1.1, vec![click(true), click(false)]);
    if caption {
        first
    } else {
        assert!(
            first.is_empty(),
            "a single click must not resize the window"
        );
        frame(1.2, vec![click(true), click(false)])
    }
}

#[test]
fn caption_enters_fullscreen_on_macos_even_when_maximized() {
    for maximized in [false, true] {
        let commands = title_bar_clicks(maximized, true);
        if cfg!(target_os = "macos") {
            assert_eq!(commands, [egui::ViewportCommand::Fullscreen(true)]);
        } else {
            assert_eq!(commands, [egui::ViewportCommand::Maximized(!maximized)]);
        }
    }
}

#[test]
fn drag_bar_double_click_maximizes_and_restores_without_fullscreen() {
    for maximized in [false, true] {
        assert_eq!(
            title_bar_clicks(maximized, false),
            [egui::ViewportCommand::Maximized(!maximized)]
        );
    }
}

fn window() -> Rect {
    Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 860.0))
}

#[test]
fn the_edges_do_not_overlap() {
    let bands = edges(window());
    for (i, (_, a)) in bands.iter().enumerate() {
        for (_, b) in bands.iter().skip(i + 1) {
            assert!(!a.intersects(b.shrink(0.01)), "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn the_edges_stay_inside_the_window() {
    for (_, band) in edges(window()) {
        assert!(window().contains_rect(band), "{band:?}");
    }
}

#[test]
fn each_corner_and_edge_resizes_its_way() {
    let at = |p| {
        edges(window())
            .into_iter()
            .find(|(_, r)| r.contains(p))
            .map(|(d, _)| d)
    };
    assert_eq!(at(pos2(1.0, 1.0)), Some(ResizeDirection::NorthWest));
    assert_eq!(at(pos2(1399.0, 859.0)), Some(ResizeDirection::SouthEast));
    assert_eq!(at(pos2(700.0, 1.0)), Some(ResizeDirection::North));
    assert_eq!(at(pos2(1.0, 430.0)), Some(ResizeDirection::West));
    assert_eq!(at(pos2(1398.0, 430.0)), Some(ResizeDirection::East));
    assert_eq!(at(pos2(700.0, 430.0)), None);
}

#[test]
fn supported_platforms_replace_the_title_bar_by_default() {
    let supported = cfg!(any(windows, target_os = "macos"));
    assert_eq!(wanted(&Settings::default()), supported);
    let off = Settings {
        tabs_in_title_bar: false,
        ..Settings::default()
    };
    assert!(!wanted(&off));
}

#[test]
fn custom_title_bar_keeps_the_native_macos_frame() {
    let custom = viewport(false);
    if cfg!(target_os = "macos") {
        assert_eq!(custom.decorations, Some(true));
        assert_eq!(custom.fullsize_content_view, Some(true));
        assert_eq!(custom.titlebar_shown, Some(false));
        assert_eq!(custom.title_shown, Some(false));
        assert_eq!(custom.titlebar_buttons_shown, Some(false));
        let native = viewport(true);
        assert_eq!(native.fullsize_content_view, Some(false));
        assert_eq!(native.titlebar_buttons_shown, Some(true));
    } else {
        assert_eq!(custom.decorations, Some(false));
    }
}

#[test]
fn the_setting_is_read_from_what_eframe_saved() {
    let saved = r#"{"app":"(open:[],settings:(tabs_in_title_bar:false))","egui":"()"}"#;
    assert!(saved_settings(saved).is_some_and(|s| !s.tabs_in_title_bar));
    let unsaved = r#"{"app":"(settings:(ui_scale:1.25))"}"#;
    assert!(saved_settings(unsaved).is_some_and(|s| s.tabs_in_title_bar));
    assert!(saved_settings("not ron").is_none());
    assert!(saved_settings(r#"{"egui":"()"}"#).is_none());
}

#[test]
fn a_window_with_nothing_saved_follows_the_default() {
    let decorated = !cfg!(any(windows, target_os = "macos"));
    assert_eq!(decorated_by(None), decorated);
    assert_eq!(
        decorated_by(Some(Path::new("/nonexistent/gitcraft/app.ron"))),
        decorated
    );
}

#[test]
fn the_title_bar_is_switched_once_per_change() {
    let ctx = egui::Context::default();
    let mut settings = Settings {
        tabs_in_title_bar: false,
        ..Settings::default()
    };
    let mut decorated = true;
    apply(&ctx, &settings, &mut decorated);
    assert!(decorated);
    settings.tabs_in_title_bar = true;
    apply(&ctx, &settings, &mut decorated);
    assert_eq!(decorated, !cfg!(any(windows, target_os = "macos")));
    settings.tabs_in_title_bar = false;
    apply(&ctx, &settings, &mut decorated);
    assert!(decorated);
}
