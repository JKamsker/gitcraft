#[test]
fn author_hover_and_hidden_column_keep_details_and_preferences() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut styled = false;
    let mut h = egui_kittest::Harness::builder()
        .with_size([1100.0, 540.0])
        .with_step_dt(0.1)
        .build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !std::mem::replace(&mut styled, true) {
                    egui_extras::install_image_loaders(ui.ctx());
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
            },
            scene(&ctx),
        );
    let mut settings = Settings {
        refs_width: 180.0,
        author_width: 143.0,
        graph_width: 100.0,
        ..Default::default()
    };
    settings.set(&h.ctx);
    for _ in 0..5 {
        h.step();
    }
    let mut texts = Vec::new();
    for shape in &h.output().shapes {
        collect_text(&shape.shape, shape.clip_rect, &mut texts);
    }
    let name = texts
        .iter()
        .find(|(_, text, _)| text.starts_with("A very long author"));
    assert!(name.is_some());
    let at = name.map_or(egui::Pos2::ZERO, |(_, _, at)| *at + vec2(2.0, 4.0));
    h.event(egui::Event::PointerMoved(at));
    for _ in 0..20 {
        h.step();
    }
    texts.clear();
    for shape in &h.output().shapes {
        collect_text(&shape.shape, shape.clip_rect, &mut texts);
    }
    assert!(
        texts
            .iter()
            .any(|(_, text, _)| text.contains("long.author@example.com") && text.contains("Zoë")),
        "author tooltip missing: {texts:?}"
    );
    if let Ok(dir) = std::env::var("GITCRAFT_HISTORY_SHOTS") {
        h.render()
            .map_err(|e| e.to_string())?
            .save(format!("{dir}/author-tooltip.png"))
            .map_err(|e| e.to_string())?;
    }
    settings.show_author = false;
    settings.set(&h.ctx);
    h.remove_cursor();
    for _ in 0..5 {
        h.step();
    }
    texts.clear();
    for shape in &h.output().shapes {
        collect_text(&shape.shape, shape.clip_rect, &mut texts);
    }
    assert!(
        !texts
            .iter()
            .any(|(_, text, _)| text.starts_with("A very long author"))
    );
    assert_eq!(Settings::get(&h.ctx).author_width, 143.0);
    if let Ok(dir) = std::env::var("GITCRAFT_HISTORY_SHOTS") {
        h.render()
            .map_err(|e| e.to_string())?
            .save(format!("{dir}/author-hidden.png"))
            .map_err(|e| e.to_string())?;
    }
    settings.show_author = true;
    settings.set(&h.ctx);
    for _ in 0..4 {
        h.step();
    }
    assert_eq!(
        preferred_cols(
            &Settings::get(&h.ctx),
            lane_layout(&settings, h.state().graph.width)
        )
        .author,
        143.0
    );
    Ok(())
}

#[test]
fn released_divider_exchanges_with_description_and_restores_after_viewport_resize()
-> Result<(), String> {
    for width in [600, 900, 1800] {
        for percent in [100, 125, 150] {
            // Reserve the minimap strip in addition to the graph width under test.
            let ctx = egui::Context::default();
            let mut styled = false;
            let scale = percent as f32 / 100.0;
            let mut h = egui_kittest::Harness::builder()
                .with_size([(width as f32 + 84.0) * scale, 540.0 * scale])
                .build_ui_state(
                    move |ui, tab: &mut RepoTab| {
                        if !std::mem::replace(&mut styled, true) {
                            egui_extras::install_image_loaders(ui.ctx());
                            crate::theme::install_fonts(ui.ctx());
                            return;
                        }
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
                    },
                    scene(&ctx),
                );
            crate::theme::install_fonts(&h.ctx);
            h.ctx.set_zoom_factor(scale);
            let initial = Settings {
                ui_scale: scale,
                refs_width: 280.0,
                graph_width: 100.0,
                author_width: 150.0,
                when_width: 100.0,
                show_hash: false,
                show_author: true,
                show_when: true,
                show_date: false,
                ..Default::default()
            };
            initial.set(&h.ctx);
            for _ in 0..5 {
                h.step();
            }
            let find = |h: &egui_kittest::Harness<RepoTab>, label: &str| {
                let mut texts = Vec::new();
                for shape in &h.output().shapes {
                    collect_text(&shape.shape, shape.clip_rect, &mut texts);
                }
                texts
                    .into_iter()
                    .find(|(_, text, _)| text == label)
                    .map(|(clip, _, _)| clip)
                    .ok_or_else(|| format!("missing painted header: {label}"))
            };
            let viewport = h.ctx.content_rect().size();
            assert!((viewport.x - width as f32 - 84.0).abs() < 0.1);
            let before_graph = find(&h, "GRAPH")?;
            let before_desc = find(&h, "DESCRIPTION")?;
            let press = egui::pos2(before_graph.left() - 6.0, before_graph.center().y);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            h.event(egui::Event::PointerMoved(press));
            h.step();
            h.event(button(press, true));
            h.step();
            let end = press - vec2(20.0, 0.0);
            h.event(egui::Event::PointerMoved(end));
            h.step();
            h.event(button(end, false));
            h.step();
            h.event(egui::Event::PointerGone);
            for _ in 0..4 {
                h.step();
            }
            let after_graph = find(&h, "GRAPH")?;
            let after_desc = find(&h, "DESCRIPTION")?;
            // Painted clip bounds round to physical pixels. One pixel of tolerance cannot
            // mask the review's 20-point exchange into the wrong column.
            let pixel = 1.0 / scale + 0.01;
            eprintln!(
                "released {width}/{percent}: graph {before_graph:?} -> {after_graph:?}, description {before_desc:?} -> {after_desc:?}"
            );
            assert!((after_graph.left() - before_graph.left() + 20.0).abs() < pixel);
            assert!(
                (after_graph.width() - before_graph.width()).abs() < pixel,
                "Graph expanded at {width}/{percent}"
            );
            assert!(
                (after_desc.width() - before_desc.width() - 20.0).abs() < pixel,
                "Description lost the exchange at {width}/{percent}"
            );
            let saved = Settings::get(&h.ctx);
            for bar in [Bar::Graph, Bar::Hash, Bar::Author, Bar::When, Bar::Date] {
                assert_eq!(
                    preferred_cols(&saved, lane_layout(&saved, 1)).width(bar),
                    preferred_cols(&initial, lane_layout(&initial, 1)).width(bar)
                );
            }
            if let Ok(dir) = std::env::var("GITCRAFT_HISTORY_SHOTS") {
                h.render()
                    .map_err(|e| e.to_string())?
                    .save(format!("{dir}/post-drag-{width}-{percent}.png"))
                    .map_err(|e| e.to_string())?;
            }
            // Shrink far enough to consume Description's exchange first, then expand. The
            // saved reservation is an explicit preference, never overwritten by a viewport.
            h.set_size(vec2(380.0, viewport.y));
            for _ in 0..5 {
                h.step();
            }
            assert_eq!(Settings::get(&h.ctx), saved);
            h.set_size(viewport);
            for _ in 0..5 {
                h.step();
            }
            assert_eq!(Settings::get(&h.ctx), saved);
            assert!((find(&h, "GRAPH")?.width() - after_graph.width()).abs() < 0.1);
            assert!((find(&h, "DESCRIPTION")?.width() - after_desc.width()).abs() < 0.1);
            if let Ok(dir) = std::env::var("GITCRAFT_HISTORY_SHOTS") {
                h.render()
                    .map_err(|e| e.to_string())?
                    .save(format!("{dir}/restored-drag-{width}-{percent}.png"))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
