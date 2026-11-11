//! Recording-only completion and presentation tests; no Git child is started.
use super::*;
use crate::repo::DiffView;

fn key(hash: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.into(),
        path: "file.txt".into(),
    }
}

#[test]
fn latest_selection_owns_completion_and_transient_layout_until_explicit_close() {
    let files = [ChangedFile {
        path: "file.txt".into(),
        status: 'M',
        old_path: None,
        added: None,
        removed: None,
    }];
    let mut state = State::default();
    for hash in ["A", "B", "A"] {
        state.history = Some(hash.into());
        state.pending = Some(hash.into());
        let stale = if hash == "A" { "B" } else { "A" };
        for result in [None, Some(files.as_slice())] {
            assert!(completed(&mut state.pending, stale, result).is_none());
            assert_eq!(state.pending.as_deref(), Some(hash));
        }
        assert!(matches!(completed(&mut state.pending, hash, Some(&files)),
            Some(Destination::Diff(got)) if got == key(hash)));
        assert!(state.graph_with_diff(&key(hash)));
        assert!(!state.graph_with_diff(&key(stale)));
        assert!(completed(&mut state.pending, hash, Some(&files)).is_none());
    }
    state.stop();
    assert!(!state.follows_history());
    assert!(!state.graph_with_diff(&key("A")));
}

#[test]
fn hidden_or_already_selected_commit_requests_reveal_without_rewriting_diff_preferences() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/history-reveal".into(), &ctx);
    tab.commits.push(std::sync::Arc::new(crate::git::Commit {
        hash: "a".repeat(40),
        ..Default::default()
    }));
    tab.selected = Some(0); // Same-row reselect must still reopen a collapsed view.
    for split in [false, true] {
        for beside in [false, true] {
            let mut settings = crate::settings::Settings::get(&ctx);
            settings.show_detail = false;
            settings.diff_split = split;
            settings.diff_beside_graph = beside;
            settings.set(&ctx);
            tab.select(0, &ctx); // No detail: records intent without running Git.
            let selected = &tab.commits[0].hash;
            assert_eq!(
                tab.commit_changes.pending.as_deref(),
                Some(selected.as_str())
            );
            assert!(crate::settings::Settings::get(&ctx).show_detail);
            let settings = crate::settings::Settings::get(&ctx);
            assert_eq!(
                (settings.diff_split, settings.diff_beside_graph),
                (split, beside)
            );
            tab.diff = Some(DiffView::new(key(selected)));
            assert!(
                tab.diff_beside_graph(&ctx),
                "history remains navigable with automatic diff"
            );
            tab.diff = None;
        }
    }
}

#[test]
fn explicit_actions_retire_pending_reveal_and_working_tree_selection_ends_history_following() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/history-reveal".into(), &ctx);
    tab.commit_changes.history = Some("old".into());
    tab.commit_changes.pending = Some("old".into());
    tab.apply(crate::repo::Action::CloseDiff, &ctx);
    assert!(!tab.commit_changes.follows_history());
    assert!(tab.commit_changes.pending.is_none());
    tab.commit_changes.history = Some("old".into());
    tab.commits.push(std::sync::Arc::new(crate::git::Commit {
        hash: crate::repo::WIP_HASH.into(),
        ..Default::default()
    }));
    tab.select(0, &ctx);
    assert!(!tab.commit_changes.follows_history());
    assert!(tab.diff.is_none());
}

#[test]
fn automatic_reveal_renders_actual_diff_without_forcing_a_narrow_side_panel() {
    use egui_kittest::{Harness, kittest::Queryable};
    for size in [[1100.0, 800.0], [360.0, 800.0], [360.0, 230.0]] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/history-reveal-ui".into(), &ctx);
        tab.commit_changes.history = Some("a".repeat(40));
        tab.diff = Some(DiffView::new(key(&"a".repeat(40))));
        let mut styled = false;
        let mut h = Harness::builder().with_size(size).build_ui_state(
            move |ui, tab: &mut RepoTab| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(ui.ctx(), &Default::default(), &Default::default());
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings::get(ui.ctx());
                    settings.show_branches = false;
                    settings.show_detail = true;
                    settings.diff_beside_graph = false;
                    settings.set(ui.ctx());
                    styled = true;
                    return;
                }
                tab.side_panels(ui, &mut Vec::new());
                egui::CentralPanel::default().show(ui, |ui| {
                    assert!(tab.diff_beside_graph(ui.ctx()));
                    tab.graph_diff_view(ui, &mut Vec::new());
                });
            },
            tab,
        );
        for _ in 0..5 {
            h.step();
        }
        assert!(
            h.query_by_label("file.txt").is_some(),
            "selected actual diff hidden at {size:?}"
        );
        assert!(!crate::settings::Settings::get(&h.ctx).diff_beside_graph);
        if size[0] < 440.0 || size[1] < 280.0 {
            assert!(h.query_by_label("Show the detail panel").is_some());
        }
    }
}
