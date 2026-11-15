//! Graph selection must not implicitly request a file diff.
use super::*;
use crate::repo::{Action, Msg};

#[test]
fn commit_row_selection_shows_details_without_opening_a_diff() {
    for following in [false, true] {
        for same_row in [false, true] {
            for show_detail in [false, true] {
                for beside in [false, true] {
                    let ctx = egui::Context::default();
                    let mut tab = RepoTab::new_deferred("/recording-only/commit-row".into(), &ctx);
                    let hash = "a".repeat(40);
                    tab.commits.push(std::sync::Arc::new(crate::git::Commit {
                        hash: hash.clone(),
                        ..Default::default()
                    }));
                    tab.selected = same_row.then_some(0);
                    tab.detail_hash = same_row.then(|| hash.clone());
                    let mut settings = crate::settings::Settings::get(&ctx);
                    settings.show_detail = show_detail;
                    settings.diff_beside_graph = beside;
                    settings.put_for_test(&ctx);
                    // Exercise a closed diff as well as a link with a detail read in flight.
                    tab.apply(Action::CloseDiff, &ctx);
                    if following {
                        tab.commit_changes.pending = Some(hash.clone());
                        tab.commit_changes.history = Some(hash.clone());
                    }
                    crate::repo::messages::record_jobs(|| {
                        tab.apply(Action::Select(0), &ctx);
                        tab.handle(
                            Msg::Detail(hash.clone(), Ok(super::reveal_tests::detail(&hash))),
                            &ctx,
                        );
                    });
                    assert_eq!(tab.selected, Some(0));
                    assert!(tab.detail.is_some());
                    assert!(tab.diff.is_none(), "a commit-row click opened a diff");
                    assert!(tab.commit_changes.pending.is_none());
                    assert!(!tab.commit_changes.follows_history());
                    assert!(crate::settings::Settings::get(&ctx).show_detail);
                    assert_eq!(
                        crate::settings::Settings::get(&ctx).diff_beside_graph,
                        beside
                    );
                }
            }
        }
    }
}

#[test]
fn row_selection_retires_automatic_diffs_and_preserves_explicit_split_diffs() {
    for same_row in [false, true] {
        for beside in [false, true] {
            for automatic in [false, true] {
                let ctx = egui::Context::default();
                let mut tab = RepoTab::new_deferred("/recording-only/open-diff-row".into(), &ctx);
                let hash = "a".repeat(40);
                tab.commits.push(std::sync::Arc::new(crate::git::Commit {
                    hash: hash.clone(),
                    ..Default::default()
                }));
                tab.selected = same_row.then_some(0);
                let key = DiffKey::Commit {
                    hash: hash.clone(),
                    path: "file.txt".into(),
                };
                let view = crate::repo::DiffView::new(key);
                let request = view.request;
                tab.diff = Some(view);
                let mut settings = crate::settings::Settings::get(&ctx);
                settings.diff_beside_graph = beside;
                settings.put_for_test(&ctx);
                if automatic {
                    tab.commit_changes.history = Some(hash);
                }
                crate::repo::messages::record_jobs(|| tab.apply(Action::Select(0), &ctx));
                let keep = !automatic && (same_row || beside);
                assert_eq!(
                    tab.diff.as_ref().map(|view| view.request),
                    keep.then_some(request)
                );
                assert!(!tab.commit_changes.follows_history());
                assert!(crate::settings::Settings::get(&ctx).show_detail);
            }
        }
    }
}

#[test]
fn ordinary_rows_preserve_another_commits_explicit_split_diff() {
    for remerge in [false, true] {
        for ready in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/recording-only/explicit-split".into(), &ctx);
            tab.commits.push(std::sync::Arc::new(crate::git::Commit {
                hash: "b".repeat(40),
                ..Default::default()
            }));
            let mut settings = crate::settings::Settings::get(&ctx);
            settings.diff_beside_graph = true;
            settings.put_for_test(&ctx);
            let hash = "a".repeat(40);
            let path = "explicit.txt".into();
            let key = if remerge {
                DiffKey::Remerge {
                    hash,
                    path,
                    old_path: None,
                    conflicts: Vec::new(),
                }
            } else {
                DiffKey::Commit { hash, path }
            };
            let mut view = crate::repo::DiffView::new(key.clone());
            view.data = ready.then(|| Ok(Default::default()));
            let request = view.request;
            tab.diff = Some(view);
            crate::repo::messages::record_jobs(|| tab.apply(Action::Select(0), &ctx));
            assert_eq!(tab.selected_hash(), Some("b".repeat(40)));
            assert_eq!(
                tab.diff.as_ref().map(|d| (&d.key, d.request)),
                Some((&key, request))
            );
        }
    }
}

#[test]
fn branch_pill_selection_retires_another_commits_split_diff() {
    for by_hash in [false, true] {
        for same_row in [false, true] {
            retires_diff(by_hash, same_row, false, false);
        }
    }
}

#[test]
fn branch_selection_retires_completed_and_pending_remerge_diffs() {
    for ready in [false, true] {
        for same_row in [false, true] {
            retires_diff(false, same_row, true, ready);
            retires_diff(true, same_row, true, ready);
        }
    }
}

fn retires_diff(by_hash: bool, same_row: bool, remerge: bool, ready: bool) {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording-only/stale-branch-diff".into(), &ctx);
    tab.life.phase = crate::repo::lifecycle::Phase::Live;
    let target = "b".repeat(40);
    tab.commits.push(std::sync::Arc::new(crate::git::Commit {
        hash: target.clone(),
        ..Default::default()
    }));
    tab.index.insert(target.clone(), 0);
    tab.selected = same_row.then_some(0);
    tab.detail_hash = same_row.then(|| target.clone());
    let mut settings = crate::settings::Settings::get(&ctx);
    settings.diff_beside_graph = true;
    settings.put_for_test(&ctx);
    let hash = "a".repeat(40);
    let path = "previous.txt".into();
    let key = if remerge {
        DiffKey::Remerge {
            hash,
            path,
            old_path: None,
            conflicts: Vec::new(),
        }
    } else {
        DiffKey::Commit { hash, path }
    };
    let mut old = crate::repo::DiffView::new(key);
    old.data = ready.then(|| Ok(Default::default()));
    let request = old.request;
    tab.diff = Some(old);
    crate::repo::messages::record_jobs(|| {
        let action = if by_hash {
            Action::SelectBranch(target.clone())
        } else {
            Action::SelectBranchTip(0)
        };
        tab.apply(action, &ctx);
        assert!(
            tab.diff.is_none(),
            "previous diff survived branch selection"
        );
        tab.handle(Msg::Diff(request, Ok(Default::default())), &ctx);
        tab.handle(
            Msg::Detail(target.clone(), Ok(super::reveal_tests::detail(&target))),
            &ctx,
        );
    });
    assert_eq!(tab.selected, Some(0));
    assert_eq!(tab.detail.as_ref().map(|d| &d.hash), Some(&target));
    assert!(
        tab.diff.is_none(),
        "late response reopened the previous diff"
    );
    assert!(tab.commit_changes.pending.is_none());
    assert!(!tab.commit_changes.follows_history());
    assert!(crate::settings::Settings::get(&ctx).show_detail);
    assert!(crate::settings::Settings::get(&ctx).diff_beside_graph);
}

#[test]
fn branch_selection_through_history_loading_accepts_only_the_latest_detail() {
    for replace in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recording-only/paged-branch".into(), &ctx);
        tab.life.phase = crate::repo::lifecycle::Phase::Live;
        tab.has_more = true;
        let first = "b".repeat(40);
        let latest = if replace {
            "c".repeat(40)
        } else {
            first.clone()
        };
        let mut settings = crate::settings::Settings::get(&ctx);
        settings.diff_beside_graph = true;
        settings.put_for_test(&ctx);
        tab.diff = Some(crate::repo::DiffView::new(DiffKey::Remerge {
            hash: "a".repeat(40),
            path: "old.txt".into(),
            old_path: None,
            conflicts: Vec::new(),
        }));
        crate::repo::messages::record_jobs(|| {
            tab.apply(Action::SelectBranch(first.clone()), &ctx);
            assert_eq!(
                tab.pending_goto.as_ref().map(|p| p.hash()),
                Some(first.as_str())
            );
            assert!(tab.loading_more);
            if replace {
                tab.apply(Action::SelectBranch(latest.clone()), &ctx);
            }
            let page = [first.clone(), "c".repeat(40)]
                .into_iter()
                .map(|hash| crate::git::Commit {
                    hash,
                    ..Default::default()
                })
                .collect();
            let excluded = crate::repo::history_filter::rules(&ctx, tab.repo_key()).excludes();
            tab.handle(Msg::More(tab.history_gen, excluded, Ok(page)), &ctx);
            assert!(tab.pending_goto.is_none());
            assert_eq!(tab.selected_hash().as_deref(), Some(latest.as_str()));
            assert!(tab.diff.is_none());
            if replace {
                tab.handle(
                    Msg::Detail(first.clone(), Ok(super::reveal_tests::detail(&first))),
                    &ctx,
                );
                assert!(tab.detail.is_none());
            }
            tab.handle(
                Msg::Detail(latest.clone(), Ok(super::reveal_tests::detail(&latest))),
                &ctx,
            );
        });
        assert_eq!(tab.detail.as_ref().map(|d| &d.hash), Some(&latest));
        assert!(tab.diff.is_none());
    }
}
