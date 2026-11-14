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
