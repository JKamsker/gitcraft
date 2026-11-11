//! Header clicks must replace automatic layout with the user's visible choice.
use super::*;
use crate::repo::DiffView;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(
    width: f32,
    saved: bool,
    automatic: bool,
    snapshot: bool,
    ready: bool,
) -> Harness<'static, RepoTab> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/diff-layout-toggle".into(), &ctx);
    tab.snapshot_shown = snapshot;
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "a".repeat(40),
        path: "file.txt".into(),
    });
    if ready {
        view.data = Some(Ok(Default::default()));
    }
    tab.diff = Some(view);
    let mut styled = false;
    Harness::builder().with_size([width, 600.0]).build_ui_state(
        move |ui, tab: &mut RepoTab| {
            if !styled {
                crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(ui.ctx(), &Default::default(), &Default::default());
                egui_extras::install_image_loaders(ui.ctx());
                let mut settings = Settings::get(ui.ctx());
                settings.diff_beside_graph = saved;
                settings.put_for_test(ui.ctx());
                if automatic {
                    tab.reveal_selected_changes("a".repeat(40), ui.ctx());
                }
                styled = true;
                return;
            }
            // Exercise the actual diff header and action dispatcher without Git workers.
            crate::repo::messages::record_jobs(|| {
                let mut acts = Vec::new();
                egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, &mut acts));
                for action in acts {
                    tab.apply(action, ui.ctx());
                }
            });
        },
        tab,
    )
}

#[test]
fn header_toggle_uses_visible_layout_and_retires_automatic_history_following() {
    for (width, ready) in [
        (360.0, false),
        (1100.0, false),
        (360.0, true),
        (1100.0, true),
    ] {
        for saved in [false, true] {
            for (automatic, snapshot) in
                [(false, false), (false, true), (true, false), (true, true)]
            {
                let mut h = harness(width, saved, automatic, snapshot, ready);
                h.run_steps(2);
                let request = h.state().diff.as_ref().map(|view| view.request);
                let mut beside = saved || automatic;
                assert_eq!(Settings::get(&h.ctx).diff_beside_graph, saved);
                for _ in 0..3 {
                    assert_eq!(h.state().diff_beside_graph(&h.ctx), beside);
                    let tip = crate::i18n::text(if beside {
                        "repo-core-2414"
                    } else {
                        "repo-core-2415"
                    });
                    h.get_by_label(tip).click();
                    h.run_steps(2);
                    beside = !beside;
                    assert_eq!(h.state().diff_beside_graph(&h.ctx), beside);
                    assert_eq!(Settings::get(&h.ctx).diff_beside_graph, beside);
                    assert!(!h.state().commit_changes.follows_history());
                    assert_eq!(h.state().diff.as_ref().map(|view| view.request), request);
                }
            }
        }
    }
}

#[test]
fn explicit_same_file_actions_still_toggle_automatic_and_manual_diffs() {
    for saved in [false, true] {
        for automatic in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/recording-only/same-file-toggle".into(), &ctx);
            let key = DiffKey::Commit {
                hash: "a".repeat(40),
                path: "file.txt".into(),
            };
            tab.diff = Some(DiffView::new(key.clone()));
            let mut settings = Settings::get(&ctx);
            settings.diff_beside_graph = saved;
            settings.put_for_test(&ctx);
            if automatic {
                tab.reveal_selected_changes("a".repeat(40), &ctx);
            }
            crate::repo::messages::record_jobs(|| {
                tab.apply(Action::OpenDiff(key.clone()), &ctx);
            });
            assert!(tab.diff.is_none());
            assert!(!tab.commit_changes.follows_history());
            crate::repo::messages::record_jobs(|| {
                tab.apply(Action::OpenDiff(key.clone()), &ctx);
            });
            assert_eq!(tab.diff.as_ref().map(|view| &view.key), Some(&key));
            assert_eq!(tab.diff_beside_graph(&ctx), saved);
        }
    }
}

#[test]
fn history_reveal_and_explicit_layout_choices_do_not_reapply_the_profile() {
    for already_open in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recording-only/layout-profile".into(), &ctx);
        let hash = "a".repeat(40);
        tab.commits.push(std::sync::Arc::new(git::Commit {
            hash: hash.clone(),
            ..Default::default()
        }));
        if already_open {
            tab.selected = Some(0);
            tab.diff = Some(DiffView::new(DiffKey::Commit {
                hash,
                path: "file.txt".into(),
            }));
        }
        let mut settings = Settings::get(&ctx);
        settings.show_detail = false;
        settings.ui_scale = 1.25;
        settings.profile_mut().author_email = "unapplied-profile@example.invalid".into();
        settings.put_for_test(&ctx);
        ctx.set_zoom_factor(1.75);
        let (_, applied) = crate::connections::record_set_attempts(|| {
            crate::repo::messages::record_jobs(|| {
                tab.apply(Action::Select(0), &ctx);
                tab.apply(Action::DiffBesideGraph(true), &ctx);
            });
        });
        assert_eq!(
            applied, 0,
            "layout actions must not replace the connection/profile epoch"
        );
        assert_eq!(ctx.zoom_factor(), 1.75);
        let settings = Settings::get(&ctx);
        assert!(settings.show_detail && settings.diff_beside_graph);
        assert_eq!(
            settings.profile().author_email,
            "unapplied-profile@example.invalid"
        );
    }
}

#[test]
fn history_reveal_keeps_popped_diff_ownership_for_both_saved_layouts() {
    use crate::repo::popout::Panel;
    for saved in [false, true] {
        for popped in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/recording-only/popped-history-diff".into(), &ctx);
            let mut settings = Settings::get(&ctx);
            settings.diff_beside_graph = saved;
            settings.put_for_test(&ctx);
            if popped {
                tab.popouts.pop_out(Panel::Diff); // Existing window waiting for its next diff.
            }
            for hash in ["a".repeat(40), "b".repeat(40)] {
                tab.commits.push(std::sync::Arc::new(git::Commit {
                    hash: hash.clone(),
                    ..Default::default()
                }));
                tab.detail_cache.insert(&tab.refs.head, detail(hash));
            }
            for row in 0..2 {
                crate::repo::messages::record_jobs(|| tab.apply(Action::Select(row), &ctx));
                let shown = tab.diff.as_ref().map(|view| view.key.clone());
                assert_eq!(
                    shown,
                    Some(DiffKey::Commit {
                        hash: tab.commits[row].hash.clone(),
                        path: "file.txt".into()
                    })
                );
                assert!(tab.commit_changes.follows_history());
                assert_eq!(tab.docked(Panel::Diff), !popped);
                assert_eq!(tab.diff_can_split(), !popped);
                // This is layout's first diff branch; a popped owner must skip it.
                assert_eq!(tab.diff_beside_graph(&ctx), !popped);
                assert_eq!(Settings::get(&ctx).diff_beside_graph, saved);
            }
            let request = tab.diff.as_ref().map(|view| view.request);
            tab.popouts.dock(Panel::Diff);
            assert!(tab.diff_beside_graph(&ctx));
            assert_eq!(tab.diff.as_ref().map(|view| view.request), request);
        }
    }
}

fn detail(hash: String) -> git::CommitDetail {
    git::CommitDetail {
        hash,
        parents: Vec::new(),
        author: String::new(),
        email: String::new(),
        author_date: String::new(),
        committer: String::new(),
        commit_date: String::new(),
        message: "subject".into(),
        in_head: false,
        merges_after: false,
        files: vec![git::ChangedFile {
            path: "file.txt".into(),
            status: 'M',
            old_path: None,
            added: None,
            removed: None,
        }],
    }
}
