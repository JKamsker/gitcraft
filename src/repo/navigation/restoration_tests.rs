//! Restoring a recorded place must not behave like a fresh history-row selection.
use super::*;
use crate::repo::{DiffView, Msg, messages};
use crate::settings::Settings;
use std::sync::Arc;

fn commit(hash: &str) -> Arc<git::Commit> {
    Arc::new(git::Commit {
        hash: hash.into(),
        ..Default::default()
    })
}

fn key(hash: &str) -> DiffKey {
    DiffKey::Commit {
        hash: hash.into(),
        path: "file.txt".into(),
    }
}

fn detail(hash: &str) -> git::CommitDetail {
    git::CommitDetail {
        hash: hash.into(),
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

fn tab(ctx: &egui::Context, saved: bool) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/navigation-restore".into(), ctx);
    tab.life.shown(std::time::Instant::now());
    let mut settings = Settings::get(ctx);
    settings.diff_beside_graph = saved;
    settings.put_for_test(ctx);
    tab.refs.head = "a".repeat(40);
    tab.history = vec![commit(&"a".repeat(40)), commit(&"b".repeat(40))];
    tab.rebuild_rows();
    tab.selected = tab.index.get(&"a".repeat(40)).copied();
    tab
}

fn restored(tab: &RepoTab, diff: bool, selected: &str) {
    assert_eq!(tab.selected_hash().as_deref(), Some(selected));
    assert_eq!(
        tab.diff.as_ref().map(|view| view.key.clone()),
        diff.then(|| key(&"a".repeat(40)))
    );
    assert!(!tab.commit_changes.follows_history());
    assert!(tab.pending_goto.is_none());
}

fn known_case(saved: bool, automatic: bool, same: bool, cached: bool) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved);
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let target = if same { &a } else { &b };
    tab.detail = Some(detail(&a));
    if cached {
        tab.detail_cache.insert(&tab.refs.head, detail(target));
    }
    messages::record_jobs(|| {
        if automatic {
            tab.reveal_selected_changes(a.clone(), &ctx);
        } else {
            tab.diff = Some(DiffView::new(key(&a)));
        }
        tab.detail = None;
        tab.detail_hash = Some(a.clone());
        tab.nav.visit(View::Graph {
            selected: Some(target.clone()),
        });
        tab.nav.visit(View::Diff {
            key: key(&a),
            selected: Some(a.clone()),
        });
        tab.apply(Action::Navigate(-1), &ctx);
        restored(&tab, false, target);
        for hash in [&a, target] {
            tab.handle(Msg::Detail(hash.clone(), Ok(detail(hash))), &ctx);
            restored(&tab, false, target);
        }
        tab.record_view();
        assert!(matches!(tab.nav.peek(1), Some(View::Diff { .. })));
        tab.apply(Action::Navigate(1), &ctx);
        restored(&tab, true, &a);
        tab.handle(Msg::Detail(a.clone(), Ok(detail(&a))), &ctx);
        restored(&tab, true, &a);
        tab.record_view();
        assert!(matches!(tab.nav.peek(-1), Some(View::Graph { .. })));
    });
}

#[test]
fn known_row_back_and_forward_preserve_graph_and_explicit_diff_places() {
    for saved in [false, true] {
        for automatic in [false, true] {
            for same in [false, true] {
                for cached in [false, true] {
                    known_case(saved, automatic, same, cached);
                }
            }
        }
    }
}

fn delayed_case(saved: bool, diff: bool, hidden: bool, cached: bool) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved);
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    tab.commits.retain(|commit| commit.hash == a);
    tab.index.remove(&b);
    if !hidden {
        tab.history.retain(|commit| commit.hash == a);
    }
    tab.has_more = !hidden;
    if cached {
        tab.detail_cache.insert(&tab.refs.head, detail(&b));
    }
    let destination = if diff {
        View::Diff {
            key: key(&a),
            selected: Some(b.clone()),
        }
    } else {
        View::Graph {
            selected: Some(b.clone()),
        }
    };
    let start = if diff {
        View::Graph {
            selected: Some(a.clone()),
        }
    } else {
        View::Diff {
            key: key(&a),
            selected: Some(a.clone()),
        }
    };
    tab.nav.visit(destination);
    tab.nav.visit(start);
    if !diff {
        tab.diff = Some(DiffView::new(key(&a)));
    }
    messages::record_jobs(|| {
        tab.apply(Action::Navigate(-1), &ctx);
        let request = tab.diff.as_ref().map(|view| view.request);
        if !hidden {
            assert!(
                tab.pending_goto
                    .as_ref()
                    .is_some_and(|target| target.restoring() && target.hash() == b)
            );
            tab.history.push(commit(&b));
            tab.loading_more = false;
            tab.rebuild(&ctx);
        }
        restored(&tab, diff, &b);
        assert_eq!(tab.diff.as_ref().map(|view| view.request), request);
        tab.handle(Msg::Detail(b.clone(), Ok(detail(&b))), &ctx);
        restored(&tab, diff, &b);
        tab.record_view();
        assert_eq!(tab.current_view(), tab.nav.current().cloned());
        assert_eq!(Settings::get(&ctx).diff_beside_graph, saved);
    });
}

#[test]
fn hidden_and_paged_restore_targets_keep_their_intent_through_late_details() {
    for saved in [false, true] {
        for diff in [false, true] {
            for hidden in [false, true] {
                for cached in [false, true] {
                    delayed_case(saved, diff, hidden, cached);
                }
            }
        }
    }
}

#[test]
fn a_later_explicit_destination_retires_a_paged_restoration() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    for saved in [false, true] {
        let mut destinations = vec![
            Action::Select(0),
            Action::SelectHash(a.clone()),
            Action::OpenDiff(key(&a)),
            Action::OpenDiffOfLines {
                hash: a.clone(),
                path: "file.txt".into(),
                lines: Vec::new(),
            },
            Action::FileView {
                path: "file.txt".into(),
                rev: Some(a.clone()),
                tab: file_view::FileTab::File,
            },
        ];
        destinations.extend(
            crate::repo::insights::Kind::ALL
                .map(|kind| Action::Insights(crate::repo::insights::InsightsAct::Open(kind))),
        );
        destinations.extend([
            Action::Compare {
                base: a.clone(),
                base_label: "base".into(),
                target: None,
                target_label: "work".into(),
                captions: [None, None],
                since_fork: false,
            },
            Action::Story(crate::repo::review_workflow::StoryAct::OpenCode {
                commit: a.clone(),
                path: "file.txt".into(),
                line: 17,
            }),
        ]);
        destinations.extend(
            [
                crate::repo::file_comparison::Page::Migration,
                crate::repo::file_comparison::Page::CodeMoves,
                crate::repo::file_comparison::Page::Lens,
                crate::repo::file_comparison::Page::Matrix,
            ]
            .map(Action::OpenFileComparison),
        );
        for action in destinations {
            let opens_diff = matches!(
                &action,
                Action::OpenDiff(_) | Action::OpenDiffOfLines { .. }
            );
            let opens_comparison = matches!(&action, Action::Compare { .. });
            let opens_file_comparison = matches!(&action, Action::OpenFileComparison(_));
            let opens_story = matches!(&action, Action::Story(_));
            let opens_file = opens_story || matches!(&action, Action::FileView { .. });
            let opens_insights = matches!(&action, Action::Insights(_));
            let ctx = egui::Context::default();
            let mut tab = tab(&ctx, saved);
            tab.history.retain(|commit| commit.hash == a);
            tab.rebuild_rows();
            tab.has_more = true;
            messages::record_jobs(|| {
                tab.goto_target(
                    super::super::navigate::PendingGoto::Restore(b.clone()),
                    &ctx,
                );
                assert!(tab.pending_goto.is_some());
                tab.apply(action, &ctx);
                assert!(tab.pending_goto.is_none());
                assert_eq!(tab.diff.is_some(), opens_diff);
                assert_eq!(tab.compare.is_some(), opens_comparison);
                assert_eq!(tab.file_comparison.is_some(), opens_file_comparison);
                assert_eq!(tab.file_view.is_some(), opens_file);
                assert_eq!(tab.insights.is_some(), opens_insights);
                if opens_story {
                    assert!(
                        tab.file_view
                            .as_ref()
                            .is_some_and(|view| view.path == "file.txt"
                                && view.rev.as_deref() == Some(a.as_str())
                                && view.marked_line == Some(17))
                    );
                }
                let destination = tab.current_view();
                let request = tab.diff.as_ref().map(|view| view.request);
                let comparison = tab.compare.as_ref().map(|view| view.request);
                let pinned = tab
                    .file_view
                    .as_ref()
                    .map(|view| (view.path.clone(), view.rev.clone(), view.marked_line));
                let file_comparison = tab.file_comparison.is_some();
                tab.history.push(commit(&b));
                tab.rebuild(&ctx);
                tab.handle(Msg::Detail(b.clone(), Ok(detail(&b))), &ctx);
                assert_eq!(tab.selected_hash().as_deref(), Some(a.as_str()));
                assert_eq!(tab.current_view(), destination);
                assert_eq!(tab.diff.as_ref().map(|view| view.request), request);
                assert_eq!(tab.compare.as_ref().map(|view| view.request), comparison);
                assert_eq!(
                    tab.file_view.as_ref().map(|view| (
                        view.path.clone(),
                        view.rev.clone(),
                        view.marked_line
                    )),
                    pinned
                );
                assert_eq!(tab.file_comparison.is_some(), file_comparison);
            });
        }
    }
}

#[test]
fn restoring_an_existing_automatic_diff_adopts_its_request_as_explicit() {
    for saved in [false, true] {
        for ready in [false, true] {
            for same in [false, true] {
                let ctx = egui::Context::default();
                let mut tab = tab(&ctx, saved);
                let a = "a".repeat(40);
                let b = "b".repeat(40);
                tab.selected = tab.index.get(if same { &a } else { &b }).copied();
                tab.detail_hash = Some(a.clone());
                let mut view = DiffView::new(key(&a));
                if ready {
                    view.data = Some(Ok(Default::default()));
                }
                let request = view.request;
                tab.diff = Some(view);
                messages::record_jobs(|| {
                    tab.reveal_selected_changes(a.clone(), &ctx);
                    assert!(tab.commit_changes.follows_history());
                    tab.restore_navigation(
                        &View::Diff {
                            key: key(&a),
                            selected: Some(a.clone()),
                        },
                        &ctx,
                    );
                    restored(&tab, true, &a);
                    assert_eq!(tab.diff_beside_graph(&ctx), saved);
                    tab.handle(Msg::Detail(a.clone(), Ok(detail(&a))), &ctx);
                    assert_eq!(tab.diff.as_ref().map(|view| view.request), Some(request));
                    restored(&tab, true, &a);
                });
            }
        }
    }
}

#[test]
fn focusing_the_commit_box_retires_paged_navigation_without_reapplying_the_profile() {
    for saved in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx, saved);
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        tab.history.retain(|commit| commit.hash == a);
        tab.work.unstaged.push(git::StatusFile {
            path: "file.txt".into(),
            old_path: None,
            code: 'M',
            area: git::Area::Unstaged,
        });
        tab.rebuild_rows();
        tab.selected = tab.index.get(&a).copied();
        tab.has_more = true;
        Settings::set_show_detail(&ctx, false);
        let (_, applied) = crate::connections::record_set_attempts(|| {
            messages::record_jobs(|| {
                tab.goto_target(
                    super::super::navigate::PendingGoto::Restore(b.clone()),
                    &ctx,
                );
                assert!(tab.pending_goto.is_some());
                tab.apply(Action::FocusCommit, &ctx);
                assert!(tab.pending_goto.is_none());
                assert_eq!(tab.selected_hash().as_deref(), Some(WIP_HASH));
                assert!(tab.focus_commit_summary);
                tab.history.push(commit(&b));
                tab.rebuild(&ctx);
                tab.handle(Msg::Detail(b.clone(), Ok(detail(&b))), &ctx);
                assert_eq!(tab.selected_hash().as_deref(), Some(WIP_HASH));
                assert!(tab.focus_commit_summary);
                assert!(Settings::get(&ctx).show_detail);
            })
        });
        assert_eq!(applied, 0);
    }
}
