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

include!("layout_tests_widths.rs");
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

include!("layout_tests_columns.rs");
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

include!("layout_tests_growth.rs");
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

include!("layout_tests_exchange.rs");
