//! Check the painted geometry and interactions, not just the preferred-width arithmetic.

use std::sync::Arc;

use egui::epaint::{ClippedShape, TextShape};
use egui::{Event, Pos2, Rect, Shape, vec2};

use super::*;
use crate::{git, theme};

fn fixture(count: usize) -> (egui::Context, RepoTab) {
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/graph-readability-test".into(), &ctx);
    tab.commits = (0..count)
        .map(|i| {
            Arc::new(git::Commit {
                hash: format!("{i:040x}"),
                parents: if i + 1 < count {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    vec![]
                },
                summary: format!(
                    "Commit {i}: {}",
                    "Unicode 界 café 🦀 message with a long subject ".repeat(6)
                ),
                body: "A complete body.\n\nAnother paragraph with details.".to_owned(),
                refs: if i == 0 {
                    vec!["HEAD -> main".to_owned()]
                } else {
                    vec![]
                },
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    (ctx, tab)
}

fn frame(
    ctx: &egui::Context,
    tab: &mut RepoTab,
    size: egui::Vec2,
    events: Vec<Event>,
    time: f64,
) -> (Vec<ClippedShape>, Vec<Action>) {
    let mut acts = Vec::new();
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(time),
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| tab.graph_view(ui, &mut acts));
        },
    );
    out.textures_delta.clear();
    (std::mem::take(&mut out.shapes), acts)
}

fn texts(shapes: &[ClippedShape]) -> Vec<&TextShape> {
    fn visit<'a>(shape: &'a Shape, out: &mut Vec<&'a TextShape>) {
        match shape {
            Shape::Text(text) => out.push(text),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in shapes {
        visit(&shape.shape, &mut out);
    }
    out
}

fn header_x(shapes: &[ClippedShape], label: &str) -> f32 {
    let matching: Vec<_> = texts(shapes)
        .into_iter()
        .filter(|s| s.galley.job.text == label)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "missing or duplicated painted header: {label}"
    );
    matching[0].pos.x
}

#[test]
fn scrolling_reclaims_empty_refs_without_width_jitter_or_full_history_paint() {
    let (ctx, mut tab) = fixture(10_000);
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (top, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let with_pill = header_x(&top, "GRAPH");
    tab.scroll_to = Some(5000);
    let (scrolled, _) = frame(&ctx, &mut tab, size, vec![], 0.2);
    assert!(header_x(&scrolled, "GRAPH") < with_pill);
    assert_eq!(header_x(&scrolled, "GRAPH"), MIN_REFS_W + 10.0);
    assert!(texts(&scrolled).iter().any(|s| s.galley.job.text == "main"
        && s.galley.job.sections[0].format.color == Tokens::get(&ctx).text_faint));
    let summaries: Vec<_> = texts(&scrolled)
        .into_iter()
        .filter(|s| s.galley.job.text.starts_with("Commit "))
        .collect();
    assert!(
        !summaries.is_empty() && summaries.len() < 12,
        "history must remain virtualized"
    );
    for text in summaries {
        assert!(text.galley.elided);
        assert!(
            text.galley
                .rows
                .iter()
                .flat_map(|r| &r.glyphs)
                .any(|g| g.chr == '…')
        );
        assert!(
            text.visual_bounding_rect().right()
                <= size.x
                    - preferred_cols(&Settings::default(), lane_layout(&Settings::default(), 1))
                        .trailing()
        );
    }
    let (stable, _) = frame(&ctx, &mut tab, size, vec![], 0.3);
    assert_eq!(
        header_x(&stable, "DESCRIPTION"),
        header_x(&scrolled, "DESCRIPTION")
    );
    assert_eq!(Settings::get(&ctx), Settings::default());
}

#[test]
fn manual_refs_optional_columns_and_compact_graph_survive_auto_sizing() {
    let (ctx, mut tab) = fixture(100);
    let settings = Settings {
        refs_width: 310.0,
        compact_graph: true,
        show_hash: true,
        show_date: true,
        show_when: true,
        show_author: true,
        ..Default::default()
    };
    settings.clone().set(&ctx);
    let size = vec2(1000.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    tab.scroll_to = Some(50);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let shown = preferred_cols(&settings, lane_layout(&settings, 1))
        .squeezed(size.x, settings.last_resized);
    assert_eq!(header_x(&shapes, "GRAPH"), shown.refs + 10.0);
    for name in ["COMMIT", "AUTHOR", "WHEN", "DATE"] {
        header_x(&shapes, name);
    }
    frame(&ctx, &mut tab, vec2(220.0, 220.0), vec![], 0.2);
    assert_eq!(Settings::get(&ctx), settings);
}

#[test]
fn description_tooltip_contains_the_whole_message_and_keeps_row_clicks() {
    let (ctx, mut tab) = fixture(4);
    let size = vec2(900.0, 220.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let (shapes, _) = frame(&ctx, &mut tab, size, vec![], 0.1);
    let x = header_x(&shapes, "DESCRIPTION") + 30.0;
    let point = egui::pos2(x, 26.0 + ROW_PITCH / 2.0);
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 0.2);
    frame(&ctx, &mut tab, size, vec![], 1.5);
    let (hovered, _) = frame(&ctx, &mut tab, size, vec![], 2.0);
    let message = format!("{}\n\n{}", tab.commits[0].summary, tab.commits[0].body);
    assert!(
        texts(&hovered).iter().any(|s| s.galley.job.text == message),
        "painted texts: {:?}",
        texts(&hovered)
            .iter()
            .map(|s| &s.galley.job.text)
            .collect::<Vec<_>>()
    );
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    let (_, acts) = frame(&ctx, &mut tab, size, events, 2.1);
    assert!(acts.iter().any(|a| matches!(a, Action::Select(0))));
}

#[test]
fn lane_names_distinguish_refs_tags_detached_and_reused_columns() {
    let (ctx, mut tab) = fixture(8);
    tab.commits[3] = Arc::new(git::Commit {
        hash: "unrelated".into(),
        parents: vec!["other".into()],
        refs: vec!["origin/topic".into()],
        ..Default::default()
    });
    tab.commits[4] = Arc::new(git::Commit {
        hash: "other".into(),
        parents: vec!["tail".into()],
        ..Default::default()
    });
    tab.commits[5] = Arc::new(git::Commit {
        hash: "tail".into(),
        ..Default::default()
    });
    tab.commits[6] = Arc::new(git::Commit {
        hash: "tagged".into(),
        refs: vec!["tag: v1".into()],
        ..Default::default()
    });
    tab.commits[7] = Arc::new(git::Commit {
        hash: "detached".into(),
        ..Default::default()
    });
    tab.refs.remotes = vec![("origin".into(), vec![])];
    tab.graph = crate::graph::layout(&tab.commits);
    tab.rebuild_graph_labels();
    ctx.run_ui(Default::default(), |ui| {
        let named = tab.readable_viewport(ui, 1..3, 24);
        assert_eq!(named.labels.len(), 1);
        assert_eq!(named.labels[&1], "main");
        // A visible real pill suppresses the faint duplicate, even if it is a tag.
        assert!(tab.readable_viewport(ui, 3..5, 24).labels.is_empty());
        let tip_offscreen = tab.readable_viewport(ui, 4..6, 24);
        assert_eq!(tip_offscreen.labels[&4], "origin/topic");
        let tag_visible = tab.readable_viewport(ui, 6..7, 24);
        assert!(tag_visible.labels.is_empty());
        let detached = tab.readable_viewport(ui, 7..8, 24);
        assert_eq!(detached.labels[&7], "detached");
    })
    .drop_without_applying_deltas();
}

#[test]
fn auto_sized_pills_keep_double_click_and_branch_context_hitboxes() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).refs = vec!["feature".to_owned()];
    tab.rebuild_graph_labels();
    let size = vec2(900.0, 220.0);
    let point = egui::pos2(25.0, 26.0 + ROW_PITCH / 2.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    frame(&ctx, &mut tab, size, vec![Event::PointerMoved(point)], 0.1);
    let mut actions = Vec::new();
    for (n, pressed) in [true, false, true, false].into_iter().enumerate() {
        let (_, acts) = frame(
            &ctx,
            &mut tab,
            size,
            vec![Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            0.2 + n as f64 * 0.03,
        );
        actions.extend(acts);
    }
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Checkout(name) if name == "feature"))
    );
    let events = [true, false]
        .into_iter()
        .map(|pressed| Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect();
    frame(&ctx, &mut tab, size, events, 0.6);
    let hash = &tab.commits[0].hash;
    assert_eq!(
        ctx.data(|d| d.get_temp::<Option<String>>(super::menu::pill_menu_id(hash))),
        Some(Some("feature".to_owned()))
    );
    assert!(egui::Popup::is_any_open(&ctx));
}
