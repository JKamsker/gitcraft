use super::RepoTab;

fn tab() -> RepoTab {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/search-test".into(), &ctx);
    tab.search.open = true;
    tab
}

/// Clearing the query while a search runs: its answer is for a query that is gone, and
/// "searching…" does not stay up.
#[test]
fn clearing_the_query_ends_the_search_running() {
    let ctx = egui::Context::default();
    let mut tab = tab();
    tab.search.query = "foo".to_owned();
    tab.search.running = true;
    tab.search.query.clear();
    tab.run_search(&ctx);
    tab.search_done("foo".to_owned(), false, Ok(vec!["a".to_owned()]), &ctx);
    assert!(!tab.search.running);
    assert!(tab.search.results.is_empty());
}

/// A search closed before it answered shows no finds.
#[test]
fn a_closed_search_takes_no_answer() {
    let ctx = egui::Context::default();
    let mut tab = tab();
    tab.search.query = "foo".to_owned();
    tab.search.running = true;
    tab.toggle_search();
    tab.search_done("foo".to_owned(), false, Ok(vec!["a".to_owned()]), &ctx);
    assert!(!tab.search.running);
    assert!(tab.search.matches.is_empty());
    assert!(tab.pending_goto.is_none());
}

/// An operator written wrong is explained under the field, and nothing is searched (nor
/// left from the search before).
#[test]
fn a_query_written_wrong_says_why_and_searches_nothing() {
    let ctx = egui::Context::default();
    let mut tab = tab();
    tab.search.results = vec!["a".to_owned()];
    std::sync::Arc::make_mut(&mut tab.search.matches).insert("a".to_owned());
    tab.search.query = "fix author:".to_owned();
    tab.run_search(&ctx);
    assert!(!tab.search.running);
    assert!(tab.search.results.is_empty() && tab.search.matches.is_empty());
    let problem = tab.search.problem.clone().unwrap_or_default();
    assert!(problem.contains("author: needs a name"), "{problem}");
    // Written right, the hint goes and the search runs.
    tab.search.query = "fix author:bob".to_owned();
    tab.run_search(&ctx);
    assert!(tab.search.running);
    assert!(tab.search.problem.is_none());
}

/// What git says when a search fails (a regex it can't read) shows under the field.
#[test]
fn a_failed_search_says_why_under_the_field() {
    let ctx = egui::Context::default();
    let mut tab = tab();
    tab.search.results = vec!["a".to_owned()];
    std::sync::Arc::make_mut(&mut tab.search.matches).insert("a".to_owned());
    tab.search.query = "regex:[".to_owned();
    tab.search.running = true;
    let e = "git could not search: invalid regex".to_owned();
    tab.search_done("regex:[".to_owned(), false, Err(e.clone()), &ctx);
    assert!(tab.search.results.is_empty() && tab.search.matches.is_empty());
    // Answered right afterwards, the hint goes.
    tab.search.running = true;
    tab.search_done("regex:[".to_owned(), false, Ok(Vec::new()), &ctx);
    assert!(tab.search.problem.is_none());
    tab.search_done("regex:[".to_owned(), false, Err(e.clone()), &ctx);
    assert_eq!(tab.search.problem, Some(e));
    assert!(!tab.search.running);
    assert!(tab.status.is_none());
    tab.toggle_search();
    assert!(tab.search.problem.is_none());
}
