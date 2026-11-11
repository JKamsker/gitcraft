use super::*;
use crate::git::growth::Frame;
#[test]
fn tree_budget_refuses_small_prefix_amplification_and_observes_cancellation() {
    let data = Growth {
        baseline: [b"aaaa/bbbb/cccc/dddd/eeee".to_vec()].into_iter().collect(),
        frames: Vec::new(),
        more: false,
        shallow: false,
    };
    assert!(
        tree_rows_budget(&data, 0, &|| true, 1000, TREE_WORK)
            .is_err_and(|e| e.contains("allocation/entry budget"))
    );
    assert!(
        tree_rows_budget(&data, 0, &|| true, TREE_BYTES, 20)
            .is_err_and(|e| e.contains("work budget"))
    );
    let calls = std::cell::Cell::new(0);
    assert!(
        tree_rows(&data, 0, &|| {
            calls.set(calls.get() + 1);
            calls.get() < 4
        })
        .is_err_and(|e| e.contains("cancelled"))
    );
}
#[test]
fn component_order_keeps_subtrees_together_and_file_directory_replacement_is_explicit()
-> Result<(), String> {
    let data = Growth {
        baseline: [b"a-b".to_vec(), b"a/old".to_vec()].into_iter().collect(),
        frames: vec![
            Frame {
                hash: "base".into(),
                name: String::new(),
                date: String::new(),
                subject: String::new(),
                changes: Vec::new(),
            },
            Frame {
                hash: "tip".into(),
                name: String::new(),
                date: String::new(),
                subject: String::new(),
                changes: vec![(b"a/old".to_vec(), false), (b"a".to_vec(), true)],
            },
        ],
        more: false,
        shallow: false,
    };
    let rows = tree_rows(&data, 0, &|| true)?;
    assert_eq!(
        rows.iter().map(|r| r.path.as_slice()).collect::<Vec<_>>(),
        vec![b"a".as_slice(), b"a/old", b"a-b"]
    );
    let rows = tree_rows(&data, 1, &|| true)?;
    assert_eq!(rows.iter().filter(|r| r.path == b"a").count(), 2);
    assert!(rows.iter().any(|r| r.path == b"a" && r.directory));
    assert!(
        rows.iter()
            .any(|r| r.path == b"a" && !r.directory && r.changed && !r.removed)
    );
    assert!(rows.iter().any(|r| r.path == b"a/old" && r.removed));
    Ok(())
}
#[test]
fn real_tree_rows_keep_folders_and_deleted_contributor_hotspots() {
    let data = Growth {
        baseline: [b"src/old".to_vec()].into_iter().collect(),
        frames: vec![
            Frame {
                hash: "a".into(),
                name: "A".into(),
                date: "date".into(),
                subject: "base".into(),
                changes: Vec::new(),
            },
            Frame {
                hash: "b".into(),
                name: "B".into(),
                date: "date".into(),
                subject: "move".into(),
                changes: vec![(b"src/old".to_vec(), false), (b"src/new".to_vec(), true)],
            },
        ],
        more: false,
        shallow: false,
    };
    let rows = tree_rows(&data, 1, &|| true).unwrap_or_default();
    assert!(rows.iter().any(|r| r.path == b"src" && r.directory));
    assert!(
        rows.iter()
            .any(|r| r.path == b"src/old" && r.removed && r.changed)
    );
    assert!(
        rows.iter()
            .any(|r| r.path == b"src/new" && r.changed && !r.removed)
    );
}
#[test]
fn growth_next_frame_and_pause_open_the_displayed_commit() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/growth-ui".into(), &ctx);
    let data = Arc::new(Growth {
        baseline: [b"a".to_vec()].into_iter().collect(),
        frames: vec![
            Frame {
                hash: "a".repeat(40),
                name: "A".into(),
                date: "2026-11-06".into(),
                subject: "base".into(),
                changes: Vec::new(),
            },
            Frame {
                hash: "b".repeat(40),
                name: "B".into(),
                date: "2026-11-07".into(),
                subject: "next".into(),
                changes: vec![(b"b".to_vec(), true)],
            },
        ],
        more: false,
        shallow: false,
    });
    let rows = tree_rows(&data, 0, &|| true)?;
    tab.growth = Some(State {
        data: Some(data),
        rows,
        requested: Some(0),
        ..Default::default()
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .build_ui_state(|ui, tab: &mut RepoTab| tab.growth_window(ui.ctx()), tab);
    harness.run();
    harness.get_by_label("Next commit").click();
    harness.run_steps(2);
    let began = Instant::now();
    while harness
        .state()
        .growth
        .as_ref()
        .is_some_and(|v| v.shown != 1)
    {
        if began.elapsed() > Duration::from_secs(3) {
            return Err("Frame did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(5));
        harness.run_steps(1);
    }
    assert!(
        harness
            .state()
            .growth
            .as_ref()
            .ok_or("growth")?
            .rows
            .iter()
            .any(|r| r.path == b"b" && r.changed)
    );
    harness.get_by_label("Open this commit").click();
    harness.run();
    assert!(harness.state().growth.is_none());
    assert_eq!(harness.state().detail_hash, Some("b".repeat(40)));
    Ok(())
}
