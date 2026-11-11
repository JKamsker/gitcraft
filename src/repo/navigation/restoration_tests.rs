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
        for action in destinations {
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
                let destination = tab.current_view();
                let request = tab.diff.as_ref().map(|view| view.request);
                tab.history.push(commit(&b));
                tab.rebuild(&ctx);
                tab.handle(Msg::Detail(b.clone(), Ok(detail(&b))), &ctx);
                assert_eq!(tab.selected_hash().as_deref(), Some(a.as_str()));
                assert_eq!(tab.current_view(), destination);
                assert_eq!(tab.diff.as_ref().map(|view| view.request), request);
            });
        }
    }
}
