use super::*;
use egui::vec2;
#[test]
fn large_minimap_capture_shares_rows_edges_and_search_set() {
    use std::sync::Arc;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/minimap-capture".into(), &ctx);
    let commits: Vec<_> = (0..10_000)
        .map(|i| {
            Arc::new(crate::git::Commit {
                hash: format!("{i:040x}"),
                parents: if i < 9_999 {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    Vec::new()
                },
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&commits);
    let before = tab.graph.rows.as_ptr();
    let first_edges = tab.graph.rows[0].outgoing.as_ptr();
    let graph = tab.graph.clone();
    let matches = tab.search.matches.clone();
    assert_eq!(graph.rows.as_ptr(), before);
    assert_eq!(graph.rows[0].outgoing.as_ptr(), first_edges);
    assert!(Arc::ptr_eq(&matches, &tab.search.matches));
    assert_eq!(Arc::strong_count(&graph.rows), 2);
    // UI queue capture uses those same Arc clones; preparation changes no row.
    tab.prepare_minimap(&ctx);
    assert_eq!(tab.graph.rows.as_ptr(), before);
    assert!(!tab.history_minimap.dirty);
    assert!(Arc::strong_count(&graph.rows) >= 2);
    assert!(lane_coverage(32).contains("first 24 of 32 lanes"));
}
#[test]
fn minimap_uses_shared_focus_ring_and_keyboard_scroll() {
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/minimap-focus".into(), &ctx);
    tab.history_minimap.dirty = false;
    tab.history_minimap.rows = 100;
    tab.history_minimap.bins = vec![
        Bin {
            lanes: 1,
            ..Default::default()
        };
        100
    ];
    tab.graph.width = 32;
    let mut harness = Harness::builder().with_size([150.0, 450.0]).build_ui_state(
        |ui, tab: &mut RepoTab| {
            let rect = Rect::from_min_size(pos2(8.0, 8.0), vec2(110.0, 400.0));
            tab.history_minimap(ui, rect, 0.0, 300.0);
        },
        tab,
    );
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert!(
        harness
            .get_by_label_contains("first 24 of 32 lanes")
            .is_focused()
    );
    harness.key_press(egui::Key::PageDown);
    harness.run();
    assert_eq!(harness.state().history_minimap.offset, Some(300.0));
}
#[test]
fn actual_minimap_marks_loaded_refs_and_clicks_scroll_without_changing_selection()
-> Result<(), String> {
    use std::sync::Arc;
    let ctx = crate::theme::test_paint::context();
    ctx.enable_accesskit();
    let mut tab = RepoTab::new_deferred("/nonexistent/minimap-fixture".into(), &ctx);
    tab.commits = (0..100)
        .map(|i| {
            Arc::new(crate::git::Commit {
                hash: format!("{i:040x}"),
                parents: if i < 99 {
                    vec![format!("{:040x}", i + 1)]
                } else {
                    Vec::new()
                },
                refs: if i == 50 {
                    vec!["tag: v1".into()]
                } else {
                    Vec::new()
                },
                ..Default::default()
            })
        })
        .collect();
    tab.graph = crate::graph::layout(&tab.commits);
    tab.refs.head = tab.commits[20].hash.clone();
    std::sync::Arc::make_mut(&mut tab.search.matches).insert(tab.commits[70].hash.clone());
    tab.selected = Some(0);
    tab.has_more = true;
    let model =
        build(&tab.graph, &tab.refs.head, &tab.search.matches, &|| true).unwrap_or_else(|_| {
            Model {
                bins: Vec::new(),
                rows: 0,
            }
        });
    tab.history_minimap.bins = model.bins;
    tab.history_minimap.rows = model.rows;
    tab.history_minimap.dirty = false;
    assert!(tab.history_minimap.bins[20].head);
    assert!(tab.history_minimap.bins[50].tag);
    assert!(tab.history_minimap.bins[70].hit);
    let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(78.0, 400.0));
    let point = pos2(38.0, 300.0);
    for has_more in [false, true] {
        tab.has_more = has_more;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| tab.history_minimap(ui, rect, 0.0, 300.0),
        );
        // This headless fixture intentionally does not upload textures. Clear before
        // assertions as well, so a failure cannot panic again while unwinding.
        output.textures_delta.clear();
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .ok_or("No AccessKit update")?;
        let (_, slider) = update
            .nodes
            .iter()
            .find(|(_, node)| node.role() == egui::accesskit::Role::Slider)
            .ok_or("No minimap Slider")?;
        assert_eq!(
            slider.label(),
            Some(
                "Loaded history minimap. Shape shows all 1 lanes. Drag to scroll; arrow and Page Up/Down keys move the viewport. Blue HEAD, green tags, yellow search hits."
            )
        );
        assert_eq!(slider.numeric_value(), Some(0.0));
        assert_eq!(
            slider.max_numeric_value(),
            Some(f64::from(100.0 * ROW_PITCH - 300.0))
        );
        output.drop_without_applying_deltas();
    }
    for pressed in [true, false] {
        let events = vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ];
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            },
            |ui| tab.history_minimap(ui, rect, 0.0, 300.0),
        )
        .drop_without_applying_deltas();
    }
    assert!(
        tab.history_minimap
            .offset
            .is_some_and(|offset| offset > 1500.0)
    );
    assert_eq!(tab.selected, Some(0));
    Ok(())
}
#[test]
fn endpoints_and_drag_anchor_respect_loaded_extent() {
    assert_eq!(bin_for(99_999, 100_000, BINS), BINS - 1);
    assert_eq!(offset_at(0.0, 1000, 300.0, 150.0), 0.0);
    assert_eq!(offset_at(1.0, 1000, 300.0, 150.0), 29_700.0);
    assert_eq!(offset_at(0.5, 1000, 300.0, 50.0), 14_950.0);
    assert_eq!(offset_at(1.0, 2, 300.0, 150.0), 0.0);
}
