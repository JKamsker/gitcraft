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

/// Locate the visible ink actually painted by the graph, including any headers above it.
pub(super) fn painted_text_center(shapes: &[ClippedShape], label: &str) -> Pos2 {
    let matching: Vec<_> = texts(shapes)
        .into_iter()
        .filter(|text| text.galley.job.text == label)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "missing or duplicated painted text: {label}"
    );
    let bounds = matching[0].visual_bounding_rect();
    assert!(bounds.is_positive(), "empty painted text: {label}");
    bounds.center()
}

#[test]
fn topology_arrows_enter_history_from_uncommitted_changes_then_follow_parents() {
    let (ctx, mut tab) = fixture(3);
    Arc::make_mut(&mut tab.commits[0]).hash = WIP_HASH.into();
    tab.refs.head = tab.commits[1].hash.clone();
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(row, commit)| (commit.hash.clone(), row))
        .collect();
    tab.topology = Arc::new(crate::repo::graph_walk::Topology::build(
        &tab.commits,
        &tab.index,
        &Default::default(),
    ));
    tab.graph = crate::graph::layout(&tab.commits);
    tab.selected = Some(0);
    tab.rebuild_graph_labels();
    Settings {
        graph_lane_walk: true,
        ..Default::default()
    }
    .set(&ctx);
    let size = vec2(900.0, 600.0);
    frame(&ctx, &mut tab, size, vec![], 0.0);
    let key = |pressed| Event::Key {
        key: Key::ArrowDown,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.1);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(1))));
    // Apply only the selected cursor; opening commit details would start Git reads
    // against the deliberately nonexistent fixture repository.
    tab.selected = Some(1);
    frame(&ctx, &mut tab, size, vec![key(false)], 0.2);
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.3);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(2))));

    // A stash also has no edges in the walking topology, but must not trap focus.
    let stash_hash = "stash-fixture".to_owned();
    Arc::make_mut(&mut tab.commits[0]).hash = stash_hash.clone();
    tab.stashes.push(git::Stash {
        name: "stash@{0}".into(),
        hash: stash_hash.clone(),
        base: tab.refs.head.clone(),
        message: "saved work".into(),
        author: String::new(),
        email: String::new(),
        time: 0,
        date: String::new(),
    });
    tab.index.remove(WIP_HASH);
    tab.index.insert(stash_hash, 0);
    tab.selected = Some(0);
    tab.rebuild_graph_labels();
    frame(&ctx, &mut tab, size, vec![key(false)], 0.4);
    let (_, acts) = frame(&ctx, &mut tab, size, vec![key(true)], 0.5);
    assert!(acts.iter().any(|act| matches!(act, Action::Select(1))));
}

#[test]
fn topology_arrows_follow_the_focused_row_when_nothing_is_selected() {
    let (ctx, mut tab) = fixture(10);
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(row, commit)| (commit.hash.clone(), row))
        .collect();
    tab.topology = Arc::new(crate::repo::graph_walk::Topology::build(
        &tab.commits,
        &tab.index,
        &Default::default(),
    ));
    tab.selected = None;
    Settings {
        graph_lane_walk: true,
        ..Default::default()
    }
    .set(&ctx);
    let mut acts = Vec::new();
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![Event::Key {
                key: Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| tab.arrow_keys(ui, Some(5), &mut acts),
    );
    output.textures_delta.clear();
    assert!(acts.iter().any(|act| matches!(act, Action::Select(6))));
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

// Locate rows from painted content: the history filter strip can move the graph vertically.
fn summary_y(shapes: &[ClippedShape], summary: &str) -> f32 {
    let found = texts(shapes)
        .into_iter()
        .find(|t| t.galley.job.text == summary);
    assert!(found.is_some(), "missing painted commit summary: {summary}");
    found.map_or(0.0, |text| text.visual_bounding_rect().center().y)
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
    assert_eq!(
        header_x(&scrolled, "GRAPH"),
        MIN_REFS_W + crate::repo::columns::CELL_PAD
    );
    assert!(texts(&scrolled).iter().any(|s| s.galley.job.text == "main"
        && s.galley.job.sections[0].format.color == Tokens::get(&ctx).text_faint));
    let painted_label = scrolled.iter().find(|clipped| {
        matches!(&clipped.shape,
        Shape::Text(text) if text.galley.job.text == "main")
    });
    assert!(
        painted_label.is_some_and(|clipped| match &clipped.shape {
            Shape::Text(text) => clipped
                .clip_rect
                .contains(text.visual_bounding_rect().center()),
            _ => false,
        }),
        "the lane name must be visible, not merely painted above the viewport clip"
    );
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
    // This full-height fixture reserves 84 points for the minimap.
    let available = size.x - 84.0;
    let shown = preferred_cols(&settings, lane_layout(&settings, 1))
        .squeezed(available, settings.last_resized);
    assert_eq!(
        header_x(&shapes, "GRAPH"),
        shown.refs + crate::repo::columns::CELL_PAD
    );
    for name in ["COMMIT", "AUTHOR", "WHEN", "DATE"] {
        header_x(&shapes, name);
    }
    frame(&ctx, &mut tab, vec2(220.0, 220.0), vec![], 0.2);
    assert_eq!(Settings::get(&ctx), settings);
}

include!("readability_tests_tooltip.rs");
include!("readability_tests_lanes.rs");
