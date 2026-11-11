//! Manual navigation setup is passive even when first-file details are already ready.
use super::*;
use crate::repo::{Msg, messages};
use std::sync::Arc;

const FILE: &str = "dir with space/ünï cödé.txt";

#[derive(Clone, Copy, Debug)]
enum Readiness {
    Ready,
    Cached,
    AfterOpen,
    AfterFile,
    AfterBack,
}

const READINESS: [Readiness; 5] = [
    Readiness::Ready,
    Readiness::Cached,
    Readiness::AfterOpen,
    Readiness::AfterFile,
    Readiness::AfterBack,
];

fn detail(hash: &str, first: &str) -> git::CommitDetail {
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
            path: first.into(),
            status: 'M',
            old_path: None,
            added: None,
            removed: None,
        }],
    }
}

fn tab(ctx: &egui::Context, saved: bool, same_row: bool) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/manual-history".into(), ctx);
    tab.life.shown(std::time::Instant::now());
    below_the_graph(ctx, saved);
    tab.refs.head = "a".repeat(40);
    tab.history = ['a', 'b']
        .map(|hash| {
            Arc::new(git::Commit {
                hash: hash.to_string().repeat(40),
                ..Default::default()
            })
        })
        .to_vec();
    tab.rebuild_rows();
    tab.selected = tab
        .index
        .get(&if same_row { "a" } else { "b" }.repeat(40))
        .copied();
    tab.detail_hash = tab.selected_hash();
    tab
}

fn deliver(tab: &mut RepoTab, hash: &str, first: &str, ctx: &egui::Context) {
    tab.handle(Msg::Detail(hash.into(), Ok(detail(hash, first))), ctx);
    tab.record_view();
}

fn manual_case(saved: bool, same_row: bool, first: &str, readiness: Readiness) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved, same_row);
    let head = "a".repeat(40);
    if matches!(readiness, Readiness::Cached) {
        tab.detail_cache.insert(&head, detail(&head, first));
    } else if matches!(readiness, Readiness::Ready) && same_row {
        tab.detail = Some(detail(&head, first));
    }
    messages::record_jobs(|| {
        select_graph(&mut tab, &head, &ctx);
        if matches!(readiness, Readiness::Ready) {
            deliver(&mut tab, &head, first, &ctx);
        }
        assert_eq!(tab.nav.entries(), &[graph(Some(&head))]);
        assert!(
            tab.diff.is_none(),
            "{readiness:?}, saved={saved}, same={same_row}"
        );
        let key = commit_key(&head, FILE);
        act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
        if matches!(readiness, Readiness::AfterOpen) {
            deliver(&mut tab, &head, first, &ctx);
        }
        assert_eq!(tab.current_view(), Some(diff_at(key.clone(), Some(&head))));
        act(
            &mut tab,
            Action::FileView {
                path: FILE.into(),
                rev: Some(head.clone()),
                tab: FileTab::History,
            },
            &ctx,
        );
        if matches!(readiness, Readiness::AfterFile) {
            deliver(&mut tab, &head, first, &ctx);
        }
        assert!(tab.diff.is_none());
        assert!(tab.file_view.is_some());
        act(&mut tab, Action::Navigate(-1), &ctx);
        let request = tab.diff.as_ref().map(|view| view.request);
        if matches!(readiness, Readiness::AfterBack) {
            deliver(&mut tab, &head, first, &ctx);
        }
        assert_eq!(tab.current_view(), Some(diff_at(key.clone(), Some(&head))));
        assert_eq!(tab.diff.as_ref().map(|view| view.request), request);
        act(&mut tab, Action::Navigate(-1), &ctx);
        assert_eq!(tab.current_view(), Some(graph(Some(&head))));
        deliver(&mut tab, &head, first, &ctx);
        assert!(tab.diff.is_none(), "late detail must not reopen the graph");
        assert!(!tab.nav.can_back());
        act(&mut tab, Action::Navigate(1), &ctx);
        assert_eq!(tab.current_view(), Some(diff_at(key, Some(&head))));
        act(&mut tab, Action::Navigate(1), &ctx);
        assert!(tab.file_view.is_some());
        assert!(!tab.commit_changes.follows_history());
    });
}

#[test]
fn manual_graph_diff_file_history_is_independent_of_detail_readiness() {
    for saved in [false, true] {
        for same_row in [false, true] {
            for first in [FILE, "another first file.txt"] {
                for readiness in READINESS {
                    manual_case(saved, same_row, first, readiness);
                }
            }
        }
    }
}

fn user_case(saved: bool, by_hash: bool, same_file: bool, readiness: Readiness) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved, false);
    let head = "a".repeat(40);
    let first = if same_file {
        FILE
    } else {
        "another first file.txt"
    };
    if matches!(readiness, Readiness::Ready | Readiness::Cached) {
        tab.detail_cache.insert(&head, detail(&head, first));
    }
    messages::record_jobs(|| {
        if matches!(readiness, Readiness::Ready) {
            select_graph(&mut tab, &head, &ctx);
        }
        let action = if by_hash {
            Action::SelectHash(head.clone())
        } else {
            Action::Select(tab.index[&head])
        };
        act(&mut tab, action, &ctx);
        let ready = matches!(readiness, Readiness::Ready | Readiness::Cached);
        assert_eq!(
            tab.diff.as_ref().map(|view| view.key.clone()),
            ready.then(|| commit_key(&head, first))
        );
        let manual = commit_key(&head, FILE);
        act(&mut tab, Action::OpenDiff(manual.clone()), &ctx);
        // The explicit same-file action toggles a ready automatic diff, but opens
        // when its first file differs or the detail has not arrived yet.
        let expected = (!(ready && same_file)).then_some(manual);
        assert_eq!(tab.diff.as_ref().map(|view| view.key.clone()), expected);
        deliver(&mut tab, &head, first, &ctx);
        assert_eq!(tab.diff.as_ref().map(|view| view.key.clone()), expected);
        assert!(!tab.commit_changes.follows_history());
    });
}

#[test]
fn user_selection_and_explicit_same_file_toggle_keep_their_distinct_meanings() {
    for saved in [false, true] {
        for by_hash in [false, true] {
            for same_file in [false, true] {
                for readiness in [Readiness::Ready, Readiness::Cached, Readiness::AfterOpen] {
                    user_case(saved, by_hash, same_file, readiness);
                }
            }
        }
    }
}

fn seed_details(tab: &mut RepoTab, cached: bool) {
    if cached {
        tab.detail_cache
            .insert(&tab.refs.head, detail(&"a".repeat(40), FILE));
    }
    let mut empty = detail(&"b".repeat(40), FILE);
    empty.files.clear();
    tab.detail_cache.insert(&tab.refs.head, empty);
}

fn graph_refinement_case(saved: bool, cached: bool) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved, true);
    let (head, base) = ("a".repeat(40), "b".repeat(40));
    seed_details(&mut tab, cached);
    messages::record_jobs(|| {
        tab.record_view();
        act(&mut tab, Action::SelectHash(base.clone()), &ctx);
        act(&mut tab, Action::SelectHash(head.clone()), &ctx);
        let head_view = if cached {
            diff_at(commit_key(&head, FILE), Some(&head))
        } else {
            graph(Some(&head))
        };
        assert_eq!(tab.current_view(), Some(head_view));
        act(&mut tab, Action::Navigate(-1), &ctx);
        assert_eq!(tab.selected_hash(), Some(base.clone()));
        act(&mut tab, Action::Navigate(-1), &ctx);
        assert_eq!(tab.selected_hash(), Some(head.clone()));
        act(&mut tab, Action::Navigate(1), &ctx);
        select_graph(&mut tab, &head, &ctx);
        deliver(&mut tab, &head, FILE, &ctx);
        assert_eq!(tab.nav.entries(), &[graph(Some(&head))]);
        assert!(tab.diff.is_none());
    });
}

fn selected_diff_case(saved: bool, cached: bool) {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx, saved, true);
    let (head, base) = ("a".repeat(40), "b".repeat(40));
    let key = commit_key(&head, FILE);
    seed_details(&mut tab, cached);
    messages::record_jobs(|| {
        tab.record_view();
        act(&mut tab, Action::OpenDiff(key.clone()), &ctx);
        act(&mut tab, Action::CloseDiff, &ctx);
        select(&mut tab, &base, &ctx);
        act(&mut tab, Action::Navigate(-1), &ctx);
        assert_eq!(tab.current_view(), Some(diff_at(key.clone(), Some(&head))));
        below_the_graph(&ctx, true);
        select(&mut tab, &base, &ctx);
        assert_eq!(tab.nav.current(), Some(&diff_at(key.clone(), Some(&base))));
        act(&mut tab, Action::CloseDiff, &ctx);
        select_graph(&mut tab, &head, &ctx);
        deliver(&mut tab, &head, FILE, &ctx);
        act(&mut tab, Action::Navigate(-1), &ctx);
        assert_eq!(tab.current_view(), Some(diff_at(key, Some(&base))));
    });
}

#[test]
fn manual_graph_refinement_and_diff_selection_do_not_depend_on_cached_details() {
    for saved in [false, true] {
        for cached in [false, true] {
            graph_refinement_case(saved, cached);
            selected_diff_case(saved, cached);
        }
    }
}
