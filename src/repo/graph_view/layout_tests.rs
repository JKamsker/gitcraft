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
