//! A refresh (the window gaining the focus, or F5) reads the repository again (#195). A search
//! still being read is asked again once that reload is in, of the branches and order it read,
//! rather than retired with nothing left to answer the query in the box. A search closed,
//! cleared or already answered is not.

use std::time::Instant;

use super::*;
use crate::repo::branch_visibility::Visibility;
use crate::repo::load::Loaded;
use crate::repo::messages::record_jobs;

fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/search-refresh".into(), ctx);
    // Live, so that the refresh's reload is submitted (and recorded) rather than started.
    tab.life.shown(Instant::now());
    tab.search.open = true;
    tab
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, query: &str) -> Result<Arc<Request>, String> {
    tab.search.query = query.into();
    let (_, jobs) = record_jobs(|| tab.run_search(ctx));
    assert_eq!(jobs.len(), 1);
    tab.search.request().ok_or("missing search request".into())
}

/// The refresh's own reads are recorded and never run.
fn refresh(tab: &mut RepoTab, ctx: &egui::Context, f5: bool) {
    record_jobs(|| {
        if f5 {
            tab.refresh(ctx);
        } else {
            tab.refresh_on_focus(ctx);
        }
    });
}

/// What the reload read: no refs, and history logged from `log_from`.
fn read(tab: &RepoTab, ctx: &egui::Context, log_from: Option<Vec<String>>) -> Box<Loaded> {
    Box::new(Loaded {
        refs: Default::default(),
        history: Vec::new(),
        history_namespaces: super::super::history_filter::rules(ctx, tab.repo_key()).excludes(),
        excluded_namespace_count: Some(0),
        work: Default::default(),
        worktrees: Vec::new(),
        worktree_root: String::new(),
        stashes: Vec::new(),
        remotes: Vec::new(),
        forge_remotes: Vec::new(),
        forge_generation: tab.forge_generation,
        gerrit: None,
        submodules: Vec::new(),
        lfs: false,
        practice: None,
        identity: Default::default(),
        requested: git::LOG_PAGE,
        log_from,
        order: tab.log_order,
        read: 1,
        issues: Default::default(),
    })
}

fn reloaded(tab: &mut RepoTab, ctx: &egui::Context, result: Result<Box<Loaded>, String>) {
    record_jobs(|| tab.loaded(result, ctx));
}

fn answer(tab: &mut RepoTab, ctx: &egui::Context, request: &Arc<Request>, found: &[&str]) {
    let found = found.iter().map(|hash| (*hash).to_owned()).collect();
    let (_, jobs) = record_jobs(|| tab.handle(Msg::Search(request.clone(), Ok(found)), ctx));
    assert!(jobs.is_empty(), "a search answer submits no Git work");
}

#[test]
fn a_search_still_being_read_is_asked_again_once_the_refresh_is_read() -> Result<(), String> {
    for f5 in [false, true] {
        for failed in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = tab(&ctx);
            let before = start(&mut tab, &ctx, "before")?;
            answer(&mut tab, &ctx, &before, &["old"]);
            // A slower query is sent, and the window loses and regains the focus.
            let slow = start(&mut tab, &ctx, "slow query")?;
            refresh(&mut tab, &ctx, f5);
            assert!(
                slow.cancel.is_cancelled(),
                "the old read is still cancelled"
            );
            assert!(
                tab.search.request().is_none(),
                "asked once the reload is in"
            );
            assert!(tab.search.running, "the box still says it is searching");
            // A failed reload leaves the context as it was: the search is asked of that.
            let result = if failed {
                Err("no".into())
            } else {
                Ok(read(&tab, &ctx, None))
            };
            reloaded(&mut tab, &ctx, result);
            let again = tab
                .search
                .request()
                .ok_or("the search was not asked again")?;
            assert_eq!(again.query, "slow query", "f5={f5} failed={failed}");
            assert!(tab.search.running);
            answer(&mut tab, &ctx, &slow, &["stale"]);
            assert_eq!(tab.search.results, ["old"]);
            answer(&mut tab, &ctx, &again, &["fresh"]);
            assert!(!tab.search.running);
            assert_eq!(tab.search.searched, "slow query");
            assert_eq!(tab.search.results, ["fresh"]);
        }
    }
    Ok(())
}

#[test]
fn the_search_asked_again_searches_the_branches_the_reload_read() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    // A branch is left out of the graph, and deleted elsewhere while a search is read.
    let mut visibility = Visibility::default();
    visibility.toggle("refs/heads/gone");
    visibility.store(&ctx, tab.repo_key());
    tab.log_from = Some(vec!["HEAD".into(), "refs/heads/main".into()]);
    let slow = start(&mut tab, &ctx, "slow query")?;
    assert_eq!(slow.from, tab.log_from);
    refresh(&mut tab, &ctx, false);
    // A reload still to come (asked for meanwhile) is waited for too.
    tab.reload_again = true;
    let unchanged = read(&tab, &ctx, tab.log_from.clone());
    reloaded(&mut tab, &ctx, Ok(unchanged));
    assert!(tab.search.request().is_none(), "another read is on its way");
    // The branch is gone: every branch is shown again, so the history is logged from all.
    let all = read(&tab, &ctx, None);
    reloaded(&mut tab, &ctx, Ok(all));
    assert_eq!(tab.log_from, None);
    let again = tab
        .search
        .request()
        .ok_or("the search was not asked again")?;
    assert_eq!(again.from, None);
    answer(&mut tab, &ctx, &again, &["fresh"]);
    assert_eq!(tab.search.results, ["fresh"]);
    assert!(!tab.search.running);
    Ok(())
}

#[test]
fn a_closed_cleared_or_answered_search_is_not_run_by_a_refresh() -> Result<(), String> {
    for case in ["closed", "cleared", "answered", "closed-while-reading"] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx);
        let request = start(&mut tab, &ctx, "query")?;
        match case {
            "closed" => tab.toggle_search(),
            "cleared" => {
                tab.search.query.clear();
                record_jobs(|| tab.run_search(&ctx));
            }
            "answered" => answer(&mut tab, &ctx, &request, &["found"]),
            _ => {}
        }
        refresh(&mut tab, &ctx, false);
        if case == "closed-while-reading" {
            tab.toggle_search();
        }
        let loaded = read(&tab, &ctx, None);
        reloaded(&mut tab, &ctx, Ok(loaded));
        assert!(tab.search.request().is_none(), "{case}");
        assert!(!tab.search.running, "{case}");
        let kept: &[&str] = if case == "answered" { &["found"] } else { &[] };
        assert_eq!(tab.search.results, kept, "{case}");
    }
    Ok(())
}
