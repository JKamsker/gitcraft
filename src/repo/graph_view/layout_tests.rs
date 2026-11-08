//! Render the real history at the width/zoom matrix, including horizontal overflow.
use super::*;
use crate::repo::columns::Bar;
use std::sync::Arc;

fn scene(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/nonexistent/history-layout-test".into(), ctx);
    tab.history = (0..28)
        .map(|i| {
            Arc::new(git::Commit {
                hash: format!("{i:040x}"),
                parents: if i < 27 {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    Vec::new()
                },
                refs: if i % 7 == 0 {
                    vec![format!("feature/a-very-long-branch-name-{i}")]
                } else {
                    Vec::new()
                },
                author: "A very long author name with Unicode — Zoë".into(),
                email: "long.author@example.com".into(),
                summary: format!(
                    "History row {i}: description remains visible beside graph lanes and metadata"
                ),
                body: if i % 2 == 0 {
                    "Co-authored-by: Claude <noreply@anthropic.com>".into()
                } else {
                    String::new()
                },
                date: "2026-11-07 20:15".into(),
                time: git::now() - 300 - i * 60,
                ..Default::default()
            })
        })
        .collect();
    tab.commits = tab.history.clone();
    // Several long crossing lanes, as in S1, even with a manually constrained graph column.
    for (i, c) in tab.commits.iter_mut().enumerate().take(6) {
        Arc::make_mut(c).parents = vec![format!("{:040x}", 15 + i)];
    }
    tab.history = tab.commits.clone();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.refs.head = tab.commits[0].hash.clone();
    tab.selected = Some(3);
    tab.history_filter.excluded_total = Some(12);
    tab.history_filter.applied = Some(crate::repo::history_filter::rules(ctx, tab.repo_key()));
    tab
}

#[test]
fn history_width_and_zoom_matrix_renders_with_clipped_cells() -> Result<(), String> {
    for width in [380, 900, 1800] {
        for percent in [100, 125, 150] {
            let ctx = egui::Context::default();
            let tab = scene(&ctx);
            let mut styled = false;
            let mut h = egui_kittest::Harness::builder()
                .with_size([width as f32, 540.0])
                .build_ui_state(
                    move |ui, tab: &mut RepoTab| {
                        if !std::mem::replace(&mut styled, true) {
                            egui_extras::install_image_loaders(ui.ctx());
                            crate::theme::install_fonts(ui.ctx());
                            crate::theme::apply(
                                ui.ctx(),
                                &crate::theme::ThemeId::default(),
                                &crate::theme::Tweaks::default(),
                            );
                            return;
                        }
                        egui::CentralPanel::default()
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
                    },
                    tab,
                );
            crate::theme::install_fonts(&h.ctx);
            crate::theme::apply(
                &h.ctx,
                &crate::theme::ThemeId::default(),
                &crate::theme::Tweaks::default(),
            );
            let settings = Settings {
                refs_width: 360.0,
                graph_width: 88.0,
                show_hash: true,
                show_date: true,
                ..Default::default()
            };
            settings.set(&h.ctx);
            h.ctx.set_zoom_factor(percent as f32 / 100.0);
            for _ in 0..4 {
                h.step();
            }
            assert_eq!(h.state().commits.len(), 28);
            assert_eq!(h.state().selected, Some(3));
            let prefs = preferred_cols(&settings, lane_layout(&settings, h.state().graph.width));
            let available = width as f32 / (percent as f32 / 100.0);
            let shown = prefs.squeezed(available, None);
            let rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(shown.content_width(available), ROW_PITCH),
            );
            assert!(shown.description(rect).width() >= crate::repo::columns::MIN_DESCRIPTION_W);
            for bar in Bar::ALL {
                let cell = shown.cell(rect, bar);
                assert!(cell.width() >= bar.min());
                assert!(cell.left() >= rect.left() && cell.right() <= rect.right());
            }
            let mut texts = Vec::new();
            for shape in &h.output().shapes {
                collect_text(&shape.shape, shape.clip_rect, &mut texts);
            }
            let description = texts.iter().find(|(_, text, _)| text == "DESCRIPTION");
            let author = texts.iter().find(|(_, text, _)| text == "AUTHOR");
            if let (Some((d, _, _)), Some((a, _, _))) = (description, author) {
                assert!(d.right() <= a.left());
            }
            for (clip, text, _) in &texts {
                if text.starts_with("History row") {
                    assert!(clip.width() <= shown.description(rect).width());
                    if let Some((author, _, _)) = author {
                        assert!(clip.right() <= author.left());
                    }
                }
            }
            if let Ok(dir) = std::env::var("GITCRAFT_HISTORY_SHOTS") {
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                h.render()
                    .map_err(|e| e.to_string())?
                    .save(format!("{dir}/history-{width}-{percent}.png"))
                    .map_err(|e| e.to_string())?;
                if width == 380 {
                    h.event(egui::Event::PointerMoved(egui::pos2(150.0, 180.0)));
                    h.event(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: egui::TouchPhase::Move,
                        delta: vec2(-1000.0, 0.0),
                        modifiers: egui::Modifiers::NONE,
                    });
                    for _ in 0..8 {
                        h.step();
                    }
                    h.render()
                        .map_err(|e| e.to_string())?
                        .save(format!("{dir}/history-{width}-{percent}-right.png"))
                        .map_err(|e| e.to_string())?;
                    let pos = egui::pos2(20.0, 16.0);
                    h.event(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        h.event(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                        h.step();
                    }
                    for _ in 0..4 {
                        h.step();
                    }
                    assert!(egui::Popup::is_any_open(&h.ctx));
                    h.render()
                        .map_err(|e| e.to_string())?
                        .save(format!("{dir}/filters-{width}-{percent}.png"))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}

/// Small TableBuilder prototype, evaluating constrained cells and a virtualized custom row.
/// The production graph needs a shared row interaction and cross-cell lane/pill connectors;
/// keeping its renderer avoids TableBuilder's independent persistent resizing state.
#[test]
fn table_builder_prototype_supports_custom_paint_but_owns_another_width_state() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut cells = 0;
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 400.0),
            )),
            ..Default::default()
        },
        |ui| {
            egui_extras::TableBuilder::new(ui)
                .resizable(true)
                .column(
                    egui_extras::Column::initial(150.0)
                        .range(70.0..=800.0)
                        .clip(true),
                )
                .column(egui_extras::Column::remainder().at_least(140.0).clip(true))
                .header(26.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Refs");
                    });
                    row.col(|ui| {
                        ui.label("Description");
                    });
                })
                .body(|body| {
                    body.rows(ROW_PITCH, 1000, |mut row| {
                        row.col(|ui| {
                            cells += 1;
                            ui.painter().circle_filled(
                                ui.max_rect().center(),
                                4.0,
                                egui::Color32::RED,
                            );
                        });
                        row.col(|ui| {
                            ui.label("custom graph row");
                        });
                    })
                });
        },
    )
    .drop_without_applying_deltas();
    assert!(cells > 0 && cells < 1000);
}

fn collect_text(
    shape: &egui::Shape,
    clip: egui::Rect,
    out: &mut Vec<(egui::Rect, String, egui::Pos2)>,
) {
    match shape {
        egui::Shape::Text(text) => out.push((clip, text.galley.job.text.clone(), text.pos)),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, clip, out);
            }
        }
        _ => {}
    }
}

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
            let ctx = egui::Context::default();
            let mut styled = false;
            let scale = percent as f32 / 100.0;
            let mut h = egui_kittest::Harness::builder()
                .with_size([width as f32 * scale, 540.0 * scale])
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
            assert!((viewport.x - width as f32).abs() < 0.1);
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

fn exchange_harness(
    settings: &Settings,
    width: f32,
    short_refs: bool,
) -> egui_kittest::Harness<'static, RepoTab> {
    exchange_harness_height(settings, width, short_refs, 120.0)
}

fn exchange_harness_height(
    settings: &Settings,
    width: f32,
    short_refs: bool,
    height: f32,
) -> egui_kittest::Harness<'static, RepoTab> {
    let ctx = egui::Context::default();
    let mut tab = scene(&ctx);
    if short_refs {
        for commit in &mut tab.commits {
            Arc::make_mut(commit).refs.clear();
        }
        Arc::make_mut(&mut tab.commits[0]).refs = vec!["a".into()];
        tab.history = tab.commits.clone();
        tab.rebuild_graph_labels();
    }
    let mut styled = false;
    let mut h = egui_kittest::Harness::builder()
        // UiState supplies eight points of outer margin on each side.
        // Keep this divider matrix below the minimap height threshold: full-height
        // overview reservation is covered by the dedicated integration test.
        .with_size([
            (width + 16.0) * settings.ui_scale,
            height * settings.ui_scale,
        ])
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
            tab,
        );
    crate::theme::install_fonts(&h.ctx);
    settings.set(&h.ctx);
    h.run_steps(4);
    h
}

fn painted(h: &egui_kittest::Harness<RepoTab>) -> Result<PaintedColumns, String> {
    h.ctx
        .data(|d| d.get_temp(painted_columns_id()))
        .ok_or_else(|| "production graph renderer did not record its column geometry".to_owned())
}

fn raw_width(settings: &Settings, bar: Bar) -> f32 {
    match bar {
        Bar::Refs => settings.refs_width,
        Bar::Graph => settings.graph_width,
        Bar::Hash => settings.hash_width,
        Bar::Author => settings.author_width,
        Bar::When => settings.when_width,
        Bar::Date => settings.date_width,
    }
}

fn pointer_button(h: &mut egui_kittest::Harness<RepoTab>, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(2);
}

fn press_divider(
    h: &mut egui_kittest::Harness<RepoTab>,
    bar: Bar,
    offset: f32,
) -> Result<egui::Pos2, String> {
    // Separate gestures, including consecutive Graph gestures, are not double-clicks.
    h.event(egui::Event::PointerGone);
    h.run_steps(4);
    let geometry = painted(h)?;
    let pos = egui::pos2(
        geometry.cols.border(geometry.header, bar) + offset,
        geometry.header.center().y,
    );
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    pointer_button(h, pos, true);
    Ok(pos)
}

fn pointer_delta(
    h: &mut egui_kittest::Harness<RepoTab>,
    press: egui::Pos2,
    bar: Bar,
    change: f32,
) -> egui::Pos2 {
    let sign = if matches!(bar, Bar::Refs | Bar::Graph) {
        1.0
    } else {
        -1.0
    };
    let point = press + vec2(sign * change, 0.0);
    h.event(egui::Event::PointerMoved(point));
    h.run_steps(2);
    point
}

fn release_exchange(
    h: &mut egui_kittest::Harness<RepoTab>,
    bar: Bar,
    change: f32,
) -> Result<(), String> {
    let press = press_divider(h, bar, 0.0)?;
    let end = pointer_delta(h, press, bar, change);
    pointer_button(h, end, false);
    h.run_steps(2);
    Ok(())
}

fn exchange_settings() -> Settings {
    Settings {
        refs_width: 280.0,
        graph_width: 100.0,
        author_width: 280.0,
        when_width: 100.0,
        hash_width: 70.0,
        date_width: 100.0,
        auto_refs_width: false,
        show_hash: false,
        show_date: false,
        ..Default::default()
    }
}

#[test]
fn compressed_graph_growth_after_author_shrink_uses_description_not_refs() -> Result<(), String> {
    let mut h = exchange_harness(&exchange_settings(), 760.0, false);
    assert_eq!(painted(&h)?.cols.refs, 180.0);
    release_exchange(&mut h, Bar::Author, -130.0)?;
    let start = painted(&h)?;
    assert_eq!(start.cols.author, 150.0);
    assert_eq!(start.cols.description(start.header).width(), 270.0);
    release_exchange(&mut h, Bar::Graph, 10.0)?;
    let after = painted(&h)?;
    eprintln!("Graph sequential exchange: {start:?} -> {after:?}");
    assert_eq!(after.cols.graph, 70.0);
    assert_eq!(after.cols.refs, 180.0);
    assert_eq!(after.cols.author, 150.0);
    assert_eq!(after.cols.description(after.header).width(), 260.0);
    assert_eq!(Settings::get(&h.ctx).refs_width, 280.0);
    Ok(())
}

#[test]
fn compressed_commit_growth_after_date_shrink_uses_description_not_author() -> Result<(), String> {
    let settings = Settings {
        show_hash: true,
        show_date: true,
        ..exchange_settings()
    };
    let mut h = exchange_harness(&settings, 600.0, false);
    release_exchange(&mut h, Bar::Date, -20.0)?;
    let start = painted(&h)?;
    assert_eq!(start.cols.hash, 50.0);
    assert_eq!(start.cols.author, 80.0);
    assert_eq!(start.cols.date, 80.0);
    assert_eq!(start.cols.description(start.header).width(), 160.0);
    // A negative pointer displacement grows a trailing column.
    release_exchange(&mut h, Bar::Hash, 10.0)?;
    let after = painted(&h)?;
    eprintln!("Commit sequential exchange: {start:?} -> {after:?}");
    assert_eq!(after.cols.hash, 60.0);
    assert_eq!(after.cols.author, 80.0);
    assert_eq!(after.cols.date, 80.0);
    assert_eq!(after.cols.description(after.header).width(), 150.0);
    assert_eq!(Settings::get(&h.ctx).author_width, 280.0);
    Ok(())
}

fn assert_exchange(
    h: &egui_kittest::Harness<RepoTab>,
    start: PaintedColumns,
    saved: &Settings,
    bar: Bar,
    change: f32,
) -> Result<(), String> {
    let after = painted(h)?;
    let description = start.cols.description(start.header).width();
    let max = (start.cols.width(bar) + description - crate::repo::columns::MIN_DESCRIPTION_W)
        .min(bar.max());
    let expected = (start.cols.width(bar) + change).clamp(bar.min(), max);
    let delta = expected - start.cols.width(bar);
    for other in Bar::ALL {
        assert_eq!(
            after.cols.width(other),
            if other == bar {
                expected
            } else {
                start.cols.width(other)
            },
            "{bar:?}/{change} moved {other:?}: {start:?} -> {after:?}"
        );
        let raw = raw_width(&Settings::get(&h.ctx), other);
        assert_eq!(
            raw,
            if other == bar && delta != 0.0 {
                expected
            } else {
                raw_width(saved, other)
            },
            "{bar:?}/{change} saved {other:?}"
        );
    }
    assert_eq!(
        after.cols.description(after.header).width(),
        description - delta
    );
    let now = Settings::get(&h.ctx);
    assert_eq!(
        now.auto_refs_width,
        saved.auto_refs_width && !(bar == Bar::Refs && delta != 0.0)
    );
    if delta == 0.0 {
        assert_eq!(now.description_space, saved.description_space);
    }
    // Inspect the renderer's actual header clips too, rather than only its recorded numbers.
    let mut texts = Vec::new();
    for shape in &h.output().shapes {
        collect_text(&shape.shape, shape.clip_rect, &mut texts);
    }
    for (other, label) in [
        (Bar::Refs, "BRANCH / TAG"),
        (Bar::Graph, "GRAPH"),
        (Bar::Hash, "COMMIT"),
        (Bar::Author, "AUTHOR"),
        (Bar::When, "WHEN"),
        (Bar::Date, "DATE"),
    ] {
        let (clip, _, _) = texts
            .iter()
            .find(|(_, text, _)| text == label)
            .ok_or_else(|| format!("production renderer did not paint the {label} header"))?;
        assert!(
            (clip.width() + 12.0 - after.cols.width(other)).abs() <= 1.1 / now.ui_scale,
            "{other:?} clip {clip:?}"
        );
    }
    Ok(())
}

#[test]
fn all_bars_sequential_pointer_exchanges_preserve_siblings_and_origin_after_auto_fit()
-> Result<(), String> {
    for width in [600.0, 760.0, 1000.0, 1800.0] {
        for auto in [false, true] {
            for scale in [1.0, 1.25, 1.5] {
                for offset in [-4.0, 4.0] {
                    let settings = Settings {
                        show_hash: true,
                        show_date: true,
                        auto_refs_width: auto,
                        graph_width: if auto { 0.0 } else { 100.0 },
                        ui_scale: scale,
                        ..exchange_settings()
                    };
                    let mut h = exchange_harness(&settings, width, auto);
                    assert!((painted(&h)?.header.width() - width).abs() < 0.1);
                    if auto {
                        assert_ne!(
                            painted(&h)?.prefs.refs,
                            settings.refs_width,
                            "fixture must exercise readable_viewport's actual auto width"
                        );
                        assert_eq!(Settings::get(&h.ctx).refs_width, settings.refs_width);
                    }
                    // Refs goes last so the other five gestures retain production auto refs.
                    for bar in [
                        Bar::Graph,
                        Bar::Hash,
                        Bar::Author,
                        Bar::When,
                        Bar::Date,
                        Bar::Refs,
                    ] {
                        let geometry = painted(&h)?;
                        let donor = Bar::ALL
                            .into_iter()
                            .filter(|&other| other != bar && other != Bar::Refs)
                            .max_by(|&a, &b| {
                                (geometry.cols.width(a) - a.min())
                                    .total_cmp(&(geometry.cols.width(b) - b.min()))
                            })
                            .ok_or_else(|| {
                                "each divider must have a visible sibling donor".to_owned()
                            })?;
                        let donation = (geometry.cols.width(donor) - donor.min()).min(30.0);
                        assert!(donation >= 10.0);
                        release_exchange(&mut h, donor, -donation)?;
                        let start = painted(&h)?;
                        let saved = Settings::get(&h.ctx);
                        let grow = (start.cols.description(start.header).width()
                            - crate::repo::columns::MIN_DESCRIPTION_W)
                            .min(bar.max() - start.cols.width(bar));
                        let shrink = start.cols.width(bar) - bar.min();
                        let preferred_delta = start.prefs.width(bar) - start.cols.width(bar);
                        let press = press_divider(&mut h, bar, offset)?;
                        for change in [
                            10.0,
                            preferred_delta - 1.0,
                            preferred_delta,
                            preferred_delta + 1.0,
                            grow - 1.0,
                            grow,
                            grow + 1.0,
                            5000.0,
                            10.0,
                            0.0,
                            -1.0,
                            -shrink,
                            -shrink - 1.0,
                            -5000.0,
                            0.0,
                        ] {
                            pointer_delta(&mut h, press, bar, change);
                            assert_exchange(&h, start, &saved, bar, change)?;
                        }
                        let end = pointer_delta(&mut h, press, bar, 10.0);
                        pointer_button(&mut h, end, false);
                        assert_exchange(&h, start, &saved, bar, 10.0)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn sequential_exchange_survives_restart_visibility_and_viewport_visits() -> Result<(), String> {
    for auto in [false, true] {
        let settings = Settings {
            show_hash: true,
            show_date: true,
            auto_refs_width: auto,
            ..exchange_settings()
        };
        let mut h = exchange_harness(&settings, 760.0, auto);
        release_exchange(&mut h, Bar::Author, -80.0)?;
        release_exchange(&mut h, Bar::Graph, 10.0)?;
        let saved = Settings::get(&h.ctx);
        let original = painted(&h)?;
        assert!(saved.description_space.compression.is_some());
        let encoded = ron::to_string(&saved).map_err(|e| e.to_string())?;
        let restored: Settings = ron::from_str(&encoded).map_err(|e| e.to_string())?;
        assert_eq!(restored, saved);
        let mut restarted = exchange_harness(&restored, 760.0, auto);
        assert_eq!(painted(&restarted)?.cols, original.cols);
        // Each metadata visibility toggle is reversible without changing saved preferences
        // or the persisted allocation. A restart while hidden follows the same policy.
        for bar in [Bar::Hash, Bar::Author, Bar::When, Bar::Date] {
            let mut hidden = saved.clone();
            match bar {
                Bar::Hash => hidden.show_hash = false,
                Bar::Author => hidden.show_author = false,
                Bar::When => hidden.show_when = false,
                Bar::Date => hidden.show_date = false,
                _ => unreachable!(),
            }
            hidden.put_for_test(&restarted.ctx);
            restarted.run_steps(3);
            assert_eq!(painted(&restarted)?.cols.width(bar), 0.0);
            assert_eq!(Settings::get(&restarted.ctx), hidden);
            let encoded = ron::to_string(&hidden).map_err(|e| e.to_string())?;
            let restored: Settings = ron::from_str(&encoded).map_err(|e| e.to_string())?;
            let hidden_restart = exchange_harness(&restored, 760.0, auto);
            assert_eq!(painted(&hidden_restart)?.cols, painted(&restarted)?.cols);
            saved.put_for_test(&restarted.ctx);
            restarted.run_steps(3);
            assert_eq!(painted(&restarted)?.cols, original.cols);
        }
        let mut visits = std::collections::HashMap::new();
        for width in [
            380, 500, 600, 740, 760, 780, 1000, 1800, 600, 380, 1800, 760,
        ] {
            restarted.set_size(vec2(width as f32 + 16.0, 540.0));
            restarted.run_steps(4);
            let geometry = painted(&restarted)?;
            assert_eq!(Settings::get(&restarted.ctx), saved);
            if let Some(previous) = visits.insert(width, geometry.cols) {
                assert_eq!(geometry.cols, previous);
            }
            if width == 1800 {
                for bar in Bar::ALL {
                    assert_eq!(
                        geometry.cols.width(bar),
                        bar.bounded(geometry.prefs.width(bar))
                    );
                }
            }
            if width == 380 {
                for bar in Bar::ALL {
                    assert_eq!(geometry.cols.width(bar), bar.min());
                }
                assert_eq!(geometry.cols.description(geometry.header).width(), 140.0);
            }
        }
        assert_eq!(painted(&restarted)?.cols, original.cols);
        restarted.set_size(vec2(516.0, 540.0));
        restarted.run_steps(3);
        let blocked = painted(&restarted)?;
        assert_eq!(blocked.cols.description(blocked.header).width(), 140.0);
        for bar in Bar::ALL {
            let press = press_divider(&mut restarted, bar, 4.0)?;
            for change in [10.0, 5000.0, -10.0, -5000.0, 0.0] {
                pointer_delta(&mut restarted, press, bar, change);
                assert_exchange(&restarted, blocked, &saved, bar, change)?;
            }
            pointer_button(&mut restarted, press, false);
        }
        // Reset clears the full reservation while preserving visibility choices.
        let mut reset = saved.clone();
        reset.show_author = false;
        reset.reset_columns();
        assert_eq!(reset.description_space, Default::default());
        assert!(!reset.show_author);
    }
    Ok(())
}

#[test]
fn automatic_refs_and_graph_modes_follow_production_inputs_after_an_exchange() -> Result<(), String>
{
    let settings = Settings {
        auto_refs_width: true,
        graph_width: 0.0,
        ..exchange_settings()
    };
    let mut h = exchange_harness(&settings, 760.0, true);
    let initial = painted(&h)?;
    assert_ne!(initial.prefs.refs, settings.refs_width);
    release_exchange(&mut h, Bar::Author, -80.0)?;
    let start = painted(&h)?;
    let saved = Settings::get(&h.ctx);
    assert!(saved.fit_visible_refs());
    assert_eq!(saved.graph_width, 0.0);
    let press = press_divider(&mut h, Bar::Graph, -4.0)?;
    for change in [10.0, 5000.0, 10.0, 0.0] {
        pointer_delta(&mut h, press, Bar::Graph, change);
        let now = Settings::get(&h.ctx);
        if change == 0.0 {
            assert_eq!(now.graph_width, 0.0);
            assert_eq!(now.description_space, saved.description_space);
            assert_eq!(painted(&h)?.cols, start.cols);
        } else {
            assert!(now.graph_width > 0.0);
        }
        assert_eq!(now.refs_width, saved.refs_width);
        assert!(now.fit_visible_refs());
    }
    pointer_button(&mut h, press, false);
    // Actual visible-pill measurements, rather than the raw default width, remain active.
    for name in ["a-much-longer-visible-branch-name-for-auto-sizing", "a"] {
        Arc::make_mut(&mut h.state_mut().commits[0]).refs = vec![name.into()];
        h.state_mut().rebuild_graph_labels();
        h.run_steps(4);
        assert_eq!(Settings::get(&h.ctx).refs_width, settings.refs_width);
        assert_eq!(Settings::get(&h.ctx).graph_width, 0.0);
        assert!(Settings::get(&h.ctx).fit_visible_refs());
        if name == "a" {
            assert_eq!(painted(&h)?.prefs.refs, initial.prefs.refs);
            assert_eq!(painted(&h)?.cols, start.cols);
        } else {
            assert!(painted(&h)?.prefs.refs > initial.prefs.refs);
        }
    }
    // A lane-fit change also remains automatic and is reversible with the same reservation.
    let width = h.state().graph.width;
    h.state_mut().graph.width = 1;
    h.run_steps(3);
    assert_eq!(Settings::get(&h.ctx).graph_width, 0.0);
    assert!(painted(&h)?.prefs.graph < start.prefs.graph);
    h.state_mut().graph.width = width;
    h.run_steps(3);
    assert_eq!(painted(&h)?.cols, start.cols);
    Ok(())
}

#[test]
fn full_height_minimap_reservation_keeps_visible_date_divider_interactive() -> Result<(), String> {
    let settings = Settings {
        show_hash: true,
        show_date: true,
        ..exchange_settings()
    };
    let mut h = exchange_harness_height(&settings, 1100.0, false, 540.0);
    let start = painted(&h)?;
    assert_eq!(
        start.header.width(),
        1016.0,
        "the minimap owns its separate 84-point strip"
    );
    let saved = Settings::get(&h.ctx);
    let press = press_divider(&mut h, Bar::Date, -4.0)?;
    let end = pointer_delta(&mut h, press, Bar::Date, 10.0);
    pointer_button(&mut h, end, false);
    assert_exchange(&h, start, &saved, Bar::Date, 10.0)?;
    Ok(())
}
