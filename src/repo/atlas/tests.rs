use super::*;
#[test]
fn non_utf8_hotspot_opens_exact_byte_history_and_immutable_commit() -> Result<(), String> {
    use crate::git::activity::{History, Record};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Arc;
    let day = Day::parse("2026-11-08").ok_or("day")?;
    let history = Arc::new(History {
        commits: [
            (b"f\xff".to_vec(), "touch-ff", "a".repeat(40)),
            (b"f\xfe".to_vec(), "touch-fe", "b".repeat(40)),
        ]
        .into_iter()
        .map(|(path, subject, hash)| Record {
            hash,
            name: "Author".into(),
            email: String::new(),
            author: day,
            committer: day,
            subject: subject.into(),
            paths: [path].into_iter().collect(),
        })
        .collect(),
        tips: 2,
        ..Default::default()
    });
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/byte-atlas".into(), &ctx);
    tab.atlas = Some(State {
        data: Some(Atlas {
            head: "head".into(),
            history,
            nodes: vec![
                atlas::Node {
                    children: vec![1],
                    directory: true,
                    ..Default::default()
                },
                atlas::Node {
                    path: b"f\xff".to_vec(),
                    revision: Some("side".into()),
                    changes: 1,
                    ..Default::default()
                },
            ],
            selected: 2,
            query: Query::default(),
        }),
        ..Default::default()
    });
    let mut harness = Harness::builder()
        .with_size([1200.0, 1000.0])
        .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
    harness.run();
    harness.get_by_label_contains(r"f\xff (file)").click();
    harness.run_steps(2);
    let start = std::time::Instant::now();
    while harness
        .state()
        .atlas
        .as_ref()
        .and_then(|v| v.byte_history.as_ref())
        .is_none_or(|h| h.rows.is_empty())
    {
        if start.elapsed() > std::time::Duration::from_secs(5) {
            return Err("Byte path history did not finish".into());
        }
        std::thread::yield_now();
        harness.run_steps(1);
    }
    assert_eq!(
        harness
            .state()
            .atlas
            .as_ref()
            .and_then(|v| v.byte_history.as_ref())
            .map(|h| h.rows.as_slice()),
        Some([0].as_slice())
    );
    harness.get_by_label_contains("touch-ff").click();
    harness.run_steps(2);
    assert!(harness.state().atlas.is_none());
    assert_eq!(harness.state().detail_hash, Some("a".repeat(40)));
    Ok(())
}
#[test]
fn overlapping_file_and_historical_folder_both_remain_reachable() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    for folder in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/atlas-overlap".into(), &ctx);
        tab.atlas = Some(State {
            size: Size::Changes,
            data: Some(Atlas {
                head: "head".into(),
                history: std::sync::Arc::new(crate::git::activity::History::default()),
                selected: 2,
                query: Query::default(),
                nodes: vec![
                    atlas::Node {
                        directory: true,
                        children: vec![1, 2],
                        ..Default::default()
                    },
                    atlas::Node {
                        path: b"a".to_vec(),
                        bytes: 100,
                        changes: 1,
                        revision: Some("head".into()),
                        ..Default::default()
                    },
                    atlas::Node {
                        path: b"a".to_vec(),
                        directory: true,
                        children: vec![3],
                        changes: 1,
                        ..Default::default()
                    },
                    atlas::Node {
                        path: b"a/old".to_vec(),
                        changes: 1,
                        revision: Some("side".into()),
                        ..Default::default()
                    },
                ],
            }),
            ..Default::default()
        });
        let mut harness = Harness::builder()
            .with_size(vec2(1200.0, 900.0))
            .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
        harness.run();
        harness
            .get_by_label_contains(if folder { "a (folder)" } else { "a (file)" })
            .click();
        harness.run();
        if folder {
            assert_eq!(harness.state().atlas.as_ref().ok_or("folder")?.zoom, 2);
            harness.get_by_label_contains("a/old (file)").click();
            harness.run();
        }
        let file = harness.state().file_view.as_ref().ok_or("file history")?;
        assert_eq!(file.path, if folder { "a/old" } else { "a" });
        assert_eq!(
            file.rev.as_deref(),
            Some(if folder { "side" } else { "head" })
        );
    }
    Ok(())
}
#[test]
fn treemap_covers_area_once_and_keeps_weights() {
    let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(100.0, 100.0));
    let mut out = Vec::new();
    partition(rect, &[1.0, 3.0, 6.0], 0, &mut out);
    assert_eq!(out.len(), 3);
    for (i, r) in &out {
        assert!((r.area() / rect.area() - [0.1, 0.3, 0.6][*i]).abs() < 0.001);
    }
    assert!(!out[0].1.intersects(out[2].1.shrink(0.01)));
}
#[test]
fn atlas_folder_hotspot_zooms_and_file_hotspot_opens_pinned_history() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/atlas-ui".into(), &ctx);
    let nodes = vec![
        atlas::Node {
            children: vec![1],
            bytes: 4,
            ..Default::default()
        },
        atlas::Node {
            path: b"src".to_vec(),
            children: vec![2],
            directory: true,
            bytes: 4,
            ..Default::default()
        },
        atlas::Node {
            path: b"src/a".to_vec(),
            revision: Some("a".repeat(40).into()),
            bytes: 4,
            ..Default::default()
        },
    ];
    tab.atlas = Some(State {
        data: Some(Atlas {
            head: "a".repeat(40),
            history: std::sync::Arc::new(crate::git::activity::History::default()),
            nodes,
            selected: 0,
            query: Query::default(),
        }),
        ..Default::default()
    });
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .build_ui_state(|ui, tab: &mut RepoTab| tab.atlas_window(ui.ctx()), tab);
    harness.run();
    harness
        .get_by_label_contains("src (folder), 4 bytes")
        .click();
    harness.run();
    assert_eq!(harness.state().atlas.as_ref().ok_or("atlas")?.zoom, 1);
    harness
        .get_by_label_contains("src/a (file), 4 bytes")
        .click();
    harness.run();
    assert!(harness.state().atlas.is_none());
    let file = harness.state().file_view.as_ref().ok_or("file history")?;
    assert_eq!(file.path, "src/a");
    assert_eq!(file.rev, Some("a".repeat(40)));
    Ok(())
}
