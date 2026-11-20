//! A refresh (the window gaining the focus, or F5) reads the repository again (#195). A search
//! still being read is asked again of what is read now, rather than retired with nothing left
//! to answer the query in the box; a search closed, cleared or already answered is not.

use super::*;
use crate::repo::messages::record_jobs;

fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/search-refresh".into(), ctx);
    tab.search.open = true;
    tab
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, query: &str) -> Result<Arc<Request>, String> {
    tab.search.query = query.into();
    let (_, jobs) = record_jobs(|| tab.run_search(ctx));
    assert_eq!(jobs.len(), 1);
    tab.search.request().ok_or("missing search request".into())
}

/// The refresh's own reads are recorded and never run: only the search's witness matters.
fn refresh(tab: &mut RepoTab, ctx: &egui::Context, f5: bool) {
    record_jobs(|| {
        if f5 {
            tab.refresh(ctx);
        } else {
            tab.refresh_on_focus(ctx);
        }
    });
}

fn answer(tab: &mut RepoTab, ctx: &egui::Context, request: &Arc<Request>, found: &[&str]) {
    let found = found.iter().map(|hash| (*hash).to_owned()).collect();
    let (_, jobs) = record_jobs(|| tab.handle(Msg::Search(request.clone(), Ok(found)), ctx));
    assert!(jobs.is_empty(), "a search answer submits no Git work");
}

#[test]
fn a_search_still_being_read_is_asked_again_after_a_refresh() -> Result<(), String> {
    for f5 in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx);
        let before = start(&mut tab, &ctx, "before")?;
        answer(&mut tab, &ctx, &before, &["old"]);
        // A slower query is typed and sent, and the window loses and regains the focus.
        let slow = start(&mut tab, &ctx, "slow query")?;
        refresh(&mut tab, &ctx, f5);
        assert!(slow.cancel.is_cancelled(), "the old read is still cancelled");
        let again = tab.search.request().ok_or("the search was not asked again")?;
        assert!(!Arc::ptr_eq(&slow, &again));
        assert!(!again.cancel.is_cancelled());
        assert_eq!(again.query, "slow query");
        assert!(tab.search.running, "f5={f5}");
        // The retired read's answer shows nothing; the new one's is the box's.
        answer(&mut tab, &ctx, &slow, &["stale"]);
        assert_eq!(tab.search.results, ["old"]);
        answer(&mut tab, &ctx, &again, &["fresh"]);
        assert!(!tab.search.running);
        assert_eq!(tab.search.searched, "slow query");
        assert_eq!(tab.search.results, ["fresh"]);
    }
    Ok(())
}

#[test]
fn a_closed_cleared_or_answered_search_is_not_run_by_a_refresh() -> Result<(), String> {
    for case in ["closed", "cleared", "answered"] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx);
        let request = start(&mut tab, &ctx, "query")?;
        match case {
            "closed" => tab.toggle_search(),
            "cleared" => {
                tab.search.query.clear();
                record_jobs(|| tab.run_search(&ctx));
            }
            _ => answer(&mut tab, &ctx, &request, &["found"]),
        }
        refresh(&mut tab, &ctx, false);
        assert!(tab.search.request().is_none(), "{case}");
        assert!(!tab.search.running, "{case}");
        let kept: &[&str] = if case == "answered" { &["found"] } else { &[] };
        assert_eq!(tab.search.results, kept, "{case}");
    }
    Ok(())
}
