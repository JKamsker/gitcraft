use super::*;
#[test]
fn release_narration_counts_only_first_parent_integrations() -> Result<(), String> {
    // feature -> main -> release; main's merge must not count twice.
    let commits: Vec<_> = [
        ("repeat", vec!["release", "feature"]),
        ("release", vec!["root", "main"]),
        ("direct", vec!["root", "feature"]),
        ("main", vec!["root", "feature"]),
        ("feature", vec!["root"]),
        ("root", vec![]),
        ("partial", vec!["unloaded", "feature"]),
    ]
    .into_iter()
    .map(|(hash, parents)| {
        std::sync::Arc::new(crate::git::Commit {
            hash: hash.into(),
            parents: parents.into_iter().map(String::from).collect(),
            ..Default::default()
        })
    })
    .collect();
    let index = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    let topology = Topology::build(&commits, &index, &HashSet::new());
    let descendants = HashSet::from([0, 1, 2, 3, 4, 6]);
    for tip in [0, 1, 2] {
        assert_eq!(
            topology.release_integrations(tip, &descendants, &commits, &|| true)?,
            (1, true)
        );
    }
    assert_eq!(
        topology.release_integrations(6, &descendants, &commits, &|| true)?,
        (0, false)
    );
    assert!(
        topology
            .release_integrations(0, &descendants, &commits, &|| false)
            .is_err()
    );
    Ok(())
}

#[test]
fn topology_toggle_requeues_cancelled_and_completed_narration() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/narration-toggle".into(), &ctx);
    tab.commits = vec![std::sync::Arc::new(crate::git::Commit {
        hash: "root".into(),
        ..Default::default()
    })];
    tab.index.insert("root".into(), 0);
    tab.selected = Some(0);
    tab.refs.local.push(crate::git::Branch {
        name: "main".into(),
        target: "root".into(),
        is_head: true,
        upstream: String::new(),
        track: String::new(),
    });
    tab.topology = std::sync::Arc::new(Topology::build(&tab.commits, &tab.index, &HashSet::new()));
    tab.graph_narration.hash = "root".into();
    let (entered, entry) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (finished, finish) = mpsc::channel();
    tab.graph_narration.reader.queue(move |active| {
        entered.send(()).map_err(|e| e.to_string())?;
        released
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        assert!(!active());
        finished.send(()).map_err(|e| e.to_string())?;
        Ok("stale result".into())
    });
    tab.graph_narration.reader.poll(&tab.tx, &ctx);
    entry
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let mut initialized = false;
    let mut harness = Harness::builder()
        .with_size([1000.0, 600.0])
        .build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !initialized {
                    crate::settings::Settings {
                        graph_lane_walk: true,
                        ..Default::default()
                    }
                    .set(ui.ctx());
                    initialized = true;
                }
                // The settings switch drives the same header lifecycle as Graph tools.
                let mut settings = crate::settings::Settings::get(ui.ctx());
                if ui
                    .checkbox(&mut settings.graph_lane_walk, "Topology setting")
                    .changed()
                {
                    settings.set(ui.ctx());
                }
                tab.graph_walk_header(ui);
            },
            tab,
        );
    harness.run_steps(2);
    harness.get_by_label("Topology setting").click();
    harness.run_steps(2);
    assert!(harness.state().graph_narration.hash.is_empty());
    release.send(()).map_err(|e| e.to_string())?;
    finish
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    for _ in 0..2 {
        harness.get_by_label("Topology setting").click();
        harness.run_steps(2);
        let start = Instant::now();
        while harness.state().graph_narration.result.is_none() {
            if start.elapsed() > Duration::from_secs(5) {
                return Err("Narration did not resume".into());
            }
            std::thread::yield_now();
            harness.run_steps(1);
        }
        assert!(
            harness
                .state()
                .graph_narration
                .result
                .as_ref()
                .is_some_and(|s| s.contains("0 ahead and 0 behind main"))
        );
        harness.get_by_label("Topology setting").click();
        harness.run_steps(2);
        assert!(harness.state().graph_narration.result.is_none());
    }
    Ok(())
}

#[test]
fn follows_edges_instead_of_adjacent_rows() {
    let commits: Vec<_> = [
        ("merge", vec!["main", "topic"]),
        ("unrelated", vec![]),
        ("topic", vec!["root"]),
        ("main", vec!["root"]),
        ("root", vec![]),
    ]
    .into_iter()
    .map(|(hash, parents)| {
        std::sync::Arc::new(crate::git::Commit {
            hash: hash.into(),
            parents: parents.into_iter().map(String::from).collect(),
            ..Default::default()
        })
    })
    .collect();
    let index = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    let topology = Topology::build(&commits, &index, &HashSet::new());
    let tips = HashSet::from([0, 1]);
    assert_eq!(topology.walk(0, Direction::Parent, &tips), Some(3));
    assert_eq!(topology.walk(2, Direction::Child, &tips), Some(0));
    assert_eq!(topology.walk(4, Direction::Merge, &tips), Some(0));
    assert_eq!(topology.walk(4, Direction::Tip, &tips), Some(0));
    assert_eq!(topology.walk(1, Direction::Parent, &tips), None);
    assert_eq!(topology.ancestors(2, &|| true), HashSet::from([2, 4]));
}
#[test]
fn lane_walking_announces_shape_and_records_only_opted_in_merge_cues() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/graph-walk-fixture".into(), &ctx);
    tab.commits = [
        ("merge", vec!["main", "topic"]),
        ("unrelated", vec![]),
        ("topic", vec!["root"]),
        ("main", vec!["root"]),
        ("root", vec![]),
    ]
    .into_iter()
    .map(|(hash, parents)| {
        std::sync::Arc::new(crate::git::Commit {
            hash: hash.into(),
            parents: parents.into_iter().map(String::from).collect(),
            ..Default::default()
        })
    })
    .collect();
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.topology = std::sync::Arc::new(Topology::build(&tab.commits, &tab.index, &HashSet::new()));
    tab.graph = crate::graph::layout(&tab.commits);
    tab.selected = Some(2);
    let _ = crate::audio::take_recorded();
    assert_eq!(tab.graph_walk(Direction::Child, None, &ctx), Some(0));
    assert!(crate::audio::take_recorded().is_empty());
    let mut settings = crate::settings::Settings::get(&ctx);
    settings.graph_sounds = true;
    settings.set(&ctx);
    assert_eq!(tab.graph_walk(Direction::Child, None, &ctx), Some(0));
    assert_eq!(
        crate::audio::take_recorded(),
        vec![crate::audio::Cue::Merge]
    );
    assert!(tab.graph_description(0).contains("merge; 2 parents"));
    assert!(tab.graph_description(4).contains("divergence"));
    tab.selected = Some(0);
    assert_eq!(tab.graph_walk(Direction::Parent, None, &ctx), Some(3));

    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    tab.rebuild_graph_labels();
    let mut initialized = false;
    let mut harness = Harness::builder().with_size([900.0, 600.0]).build_ui_state(
        move |ui, tab: &mut RepoTab| {
            if !initialized {
                crate::theme::install_fonts(ui.ctx());
                initialized = true;
                return;
            }
            crate::settings::Settings {
                graph_lane_walk: true,
                ..Default::default()
            }
            .set(ui.ctx());
            tab.graph_view(ui, &mut Vec::new());
        },
        tab,
    );
    harness.step();
    let label = harness
        .get_by_label_contains("commit merge")
        .accesskit_node()
        .label()
        .unwrap_or_default();
    assert!(label.starts_with("commit merge"), "{label}");
    assert!(label.contains("merge; 2 parents"), "{label}");
}
