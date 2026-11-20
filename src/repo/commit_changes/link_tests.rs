//! Every way of selecting a commit shows its details only; no Git child is started.
use super::*;
use crate::git::ChangedFile;
use crate::repo::{Action, Msg};

pub(super) fn detail(hash: &str) -> crate::git::CommitDetail {
    crate::git::CommitDetail {
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
        files: vec![ChangedFile {
            path: "file.txt".into(),
            status: 'M',
            old_path: None,
            added: None,
            removed: None,
        }],
    }
}

fn commit(hash: &str) -> crate::git::Commit {
    crate::git::Commit {
        hash: hash.into(),
        ..Default::default()
    }
}

/// A shown tab whose graph holds `hashes`, with the detail panel and layout as given.
fn live_tab(ctx: &egui::Context, hashes: &[&str], show_detail: bool, beside: bool) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/commit-links".into(), ctx);
    tab.life.phase = crate::repo::lifecycle::Phase::Live;
    tab.history = hashes
        .iter()
        .map(|hash| std::sync::Arc::new(commit(hash)))
        .collect();
    tab.rebuild_rows();
    let mut settings = crate::settings::Settings::get(ctx);
    settings.show_detail = show_detail;
    settings.diff_beside_graph = beside;
    settings.put_for_test(ctx);
    tab
}

fn assert_details_without_diff(tab: &RepoTab, ctx: &egui::Context, hash: &str, how: &str) {
    assert_eq!(tab.selected_hash().as_deref(), Some(hash), "{how}");
    assert_eq!(
        tab.detail.as_ref().map(|d| d.hash.as_str()),
        Some(hash),
        "{how}"
    );
    assert!(tab.diff.is_none(), "{how} opened a diff");
    assert_details_revealed(ctx, how);
}

/// The detail panel shows, even where a narrow window would fold it away.
fn assert_details_revealed(ctx: &egui::Context, how: &str) {
    assert!(crate::settings::Settings::get(ctx).show_detail, "{how}");
    assert_eq!(
        ctx.data(|data| data.get_temp::<bool>(egui::Id::new("repo_detail").with("reopen-small"))),
        Some(true),
        "{how} left a folded detail panel closed"
    );
}

/// How a selection is reached, and the action it sends for a commit hash.
type Route = (&'static str, fn(&str) -> Action);

#[test]
fn every_commit_selection_route_shows_details_without_opening_a_diff() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let routes: [Route; 5] = [
        ("a graph row", |_| Action::Select(1)),
        ("a branch pill", |_| Action::SelectBranchTip(1)),
        ("a sidebar branch", |hash| Action::SelectBranch(hash.into())),
        ("a commit link", |hash| Action::SelectHash(hash.into())),
        ("a backlink", |hash| Action::OpenCommitChanges(hash.into())),
    ];
    for (how, route) in routes {
        for show_detail in [false, true] {
            for beside in [false, true] {
                for collapsed in [false, true] {
                    let ctx = egui::Context::default();
                    let mut tab = live_tab(&ctx, &[&a, &b], show_detail, beside);
                    // The states in which selection used to reveal a diff by itself.
                    ctx.data_mut(|data| {
                        data.insert_temp(
                            egui::Id::new("repo_detail").with("auto-collapsed"),
                            collapsed,
                        )
                    });
                    crate::repo::messages::record_jobs(|| {
                        tab.apply(route(&b), &ctx);
                        assert!(tab.diff.is_none(), "{how} opened a diff before its detail");
                        tab.handle(Msg::Detail(b.clone(), Ok(detail(&b))), &ctx);
                    });
                    assert_details_without_diff(&tab, &ctx, &b, how);
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
fn cached_details_open_no_diff_either() {
    let a = "a".repeat(40);
    for by_hash in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = live_tab(&ctx, &[&a], false, false);
        tab.detail_cache.insert(&tab.refs.head, detail(&a));
        let action = if by_hash {
            Action::SelectHash(a.clone())
        } else {
            Action::OpenCommitChanges(a.clone())
        };
        crate::repo::messages::record_jobs(|| tab.apply(action, &ctx));
        assert_details_without_diff(&tab, &ctx, &a, "a cached commit link");
    }
}

#[test]
fn a_backlink_outside_the_loaded_graph_shows_its_details_without_a_diff() {
    let ctx = egui::Context::default();
    let mut tab = live_tab(&ctx, &[&"a".repeat(40)], false, false);
    let outside = "f".repeat(40);
    crate::repo::messages::record_jobs(|| {
        tab.apply(Action::OpenCommitChanges(outside.clone()), &ctx);
        tab.handle(Msg::Detail(outside.clone(), Ok(detail(&outside))), &ctx);
    });
    assert_eq!(tab.selected, None);
    assert_eq!(tab.detail.as_ref().map(|d| &d.hash), Some(&outside));
    assert!(tab.diff.is_none(), "a backlink opened a diff");
    assert_details_revealed(&ctx, "a backlink outside the graph");
}

#[test]
fn a_late_history_page_never_opens_a_diff_or_takes_back_a_clicked_selection() {
    let b = "b".repeat(40);
    let target = "c".repeat(40);
    for clicked in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = live_tab(&ctx, &[&b], false, false);
        tab.has_more = true;
        crate::repo::messages::record_jobs(|| {
            // A link to a commit further down pages history in before selecting it.
            tab.apply(Action::SelectHash(target.clone()), &ctx);
            assert_eq!(
                tab.pending_goto.as_ref().map(|p| p.hash()),
                Some(target.as_str())
            );
            if clicked {
                tab.apply(Action::Select(tab.index[&b]), &ctx);
                assert!(tab.pending_goto.is_none(), "the click left the link armed");
            }
            let excluded = crate::repo::history_filter::rules(&ctx, tab.repo_key()).excludes();
            tab.handle(
                Msg::More(tab.history_gen, excluded, Ok(vec![commit(&target)])),
                &ctx,
            );
            let shown = if clicked { &b } else { &target };
            tab.handle(Msg::Detail(shown.clone(), Ok(detail(shown))), &ctx);
        });
        let shown = if clicked { &b } else { &target };
        assert!(tab.pending_goto.is_none());
        assert_details_without_diff(&tab, &ctx, shown, "a late history page");
    }
}

#[test]
fn branch_selection_reopens_details_and_keeps_the_layout_choice() {
    for same_row in [false, true] {
        for beside in [false, true] {
            let ctx = egui::Context::default();
            let hash = "a".repeat(40);
            let mut tab = live_tab(&ctx, &[&hash], false, beside);
            tab.selected = same_row.then_some(0);
            tab.detail_hash = same_row.then(|| hash.clone());
            crate::repo::messages::record_jobs(|| {
                tab.apply(Action::SelectBranchTip(0), &ctx);
                tab.handle(Msg::Detail(hash.clone(), Ok(detail(&hash))), &ctx);
            });
            assert_details_without_diff(&tab, &ctx, &hash, "a branch pill");
            assert_eq!(
                crate::settings::Settings::get(&ctx).diff_beside_graph,
                beside
            );
            assert_eq!(
                ctx.data(
                    |data| data.get_temp::<bool>(egui::Id::new("repo_detail").with("reopen-small"))
                ),
                Some(true)
            );
        }
    }
}

#[test]
fn passive_same_row_restoration_preserves_a_closed_diff_and_detail_preference() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/passive-selection".into(), &ctx);
    tab.commits
        .push(std::sync::Arc::new(commit(&"a".repeat(40))));
    tab.selected = Some(0);
    let mut settings = crate::settings::Settings::get(&ctx);
    settings.show_detail = false;
    settings.put_for_test(&ctx);
    tab.select(0, &ctx);
    assert!(tab.diff.is_none());
    assert!(!crate::settings::Settings::get(&ctx).show_detail);
}

#[test]
fn only_a_changed_file_click_opens_the_diff() {
    let ctx = egui::Context::default();
    let hash = "a".repeat(40);
    let mut tab = live_tab(&ctx, &[&hash], true, true);
    let key = DiffKey::Commit {
        hash: hash.clone(),
        path: "file.txt".into(),
    };
    crate::repo::messages::record_jobs(|| {
        tab.apply(Action::Select(0), &ctx);
        tab.handle(Msg::Detail(hash.clone(), Ok(detail(&hash))), &ctx);
        assert!(tab.diff.is_none());
        // What the Changed Files row pushes when clicked.
        tab.apply(Action::OpenDiff(key.clone()), &ctx);
    });
    assert_eq!(tab.diff.as_ref().map(|view| &view.key), Some(&key));
}

#[test]
fn a_failed_detail_read_shows_the_error_without_opening_a_diff() {
    let ctx = egui::Context::default();
    let hash = "a".repeat(40);
    let mut tab = live_tab(&ctx, &[&hash], false, false);
    crate::repo::messages::record_jobs(|| {
        tab.apply(Action::SelectHash(hash.clone()), &ctx);
        tab.handle(Msg::Detail(hash.clone(), Err("unreadable".into())), &ctx);
    });
    assert_eq!(tab.detail_error.as_deref(), Some("unreadable"));
    assert!(tab.diff.is_none(), "a failed detail read opened a diff");
}
