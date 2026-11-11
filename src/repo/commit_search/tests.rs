use super::*;

fn tab(ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred("/recording-only/search-lifetime".into(), ctx);
    tab.search.open = true;
    tab
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, query: &str) -> Result<Arc<Request>, String> {
    tab.search.query = query.into();
    let (_, jobs) = super::super::messages::record_jobs(|| tab.run_search(ctx));
    assert_eq!(jobs.len(), 1);
    tab.search.request().ok_or("missing search request".into())
}

fn answer(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    request: &Arc<Request>,
    result: Result<Vec<String>, String>,
) {
    let (_, jobs) = super::super::messages::record_jobs(|| {
        tab.handle(Msg::Search(request.clone(), result), ctx);
    });
    assert!(
        jobs.is_empty(),
        "synthetic completion must not submit Git work"
    );
}

#[test]
fn old_successes_and_errors_cannot_settle_a_repeated_query_or_navigate() -> Result<(), String> {
    for transition in ["restart", "query", "mode", "reopen", "context"] {
        for stale_error in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = tab(&ctx);
            let old = start(&mut tab, &ctx, "same")?;
            match transition {
                "query" => {
                    start(&mut tab, &ctx, "other")?;
                }
                "mode" => {
                    tab.search.code = true;
                    start(&mut tab, &ctx, "same")?;
                    tab.search.code = false;
                }
                "reopen" => {
                    tab.toggle_search();
                    tab.toggle_search();
                }
                "context" => {
                    tab.invalidate_repository_context();
                }
                _ => {}
            }
            let current = start(&mut tab, &ctx, "same")?;
            assert!(old.cancel.is_cancelled());
            assert!(!current.cancel.is_cancelled());
            tab.search.results = vec!["displayed".into()];
            tab.search.matches = HashSet::from(["displayed".into()]).into();
            tab.search.searched = "previous".into();
            tab.history_minimap.dirty = false;
            let stale = if stale_error {
                Err("stale failure".into())
            } else {
                Ok(vec!["stale-first".into()])
            };
            for result in [stale.clone(), stale] {
                answer(&mut tab, &ctx, &old, result);
                assert!(tab.search.running, "{transition}");
                assert_eq!(tab.search.results, ["displayed"]);
                assert!(tab.search.matches.contains("displayed"));
                assert_eq!(tab.search.searched, "previous");
                assert!(tab.search.problem.is_none());
                assert!(tab.pending_goto.is_none());
                assert!(!tab.history_minimap.dirty);
            }
            answer(&mut tab, &ctx, &current, Ok(vec!["fresh".into()]));
            assert!(!tab.search.running);
            assert_eq!(tab.search.results, ["fresh"]);
            assert_eq!(tab.pending_goto.as_deref(), Some("fresh"));
            // Once consumed, even a duplicate current completion is stale.
            for request in [&old, &current] {
                answer(&mut tab, &ctx, request, Err("late failure".into()));
                assert_eq!(tab.search.results, ["fresh"]);
                assert!(tab.search.problem.is_none());
                assert_eq!(tab.pending_goto.as_deref(), Some("fresh"));
            }
        }
    }
    Ok(())
}

#[test]
fn current_errors_settle_once_and_cannot_be_erased_by_old_successes() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let old = start(&mut tab, &ctx, "same")?;
    let current = start(&mut tab, &ctx, "same")?;
    answer(&mut tab, &ctx, &current, Err("current failure".into()));
    assert!(!tab.search.running);
    assert!(tab.search.results.is_empty());
    for request in [&old, &current] {
        answer(&mut tab, &ctx, request, Ok(vec!["old-first".into()]));
        assert_eq!(tab.search.problem.as_deref(), Some("current failure"));
        assert!(tab.search.results.is_empty());
        assert!(tab.pending_goto.is_none());
    }
    Ok(())
}

#[test]
fn request_witness_checks_path_roots_order_query_mode_and_namespace() -> Result<(), String> {
    for changed in [
        "path",
        "roots",
        "order",
        "query",
        "mode",
        "namespace",
        "connection",
    ] {
        for result in [Ok(vec!["stale".into()]), Err("stale failure".into())] {
            let ctx = egui::Context::default();
            let mut tab = tab(&ctx);
            let request = start(&mut tab, &ctx, "same")?;
            match changed {
                "path" => tab.path = "/recording-only/other".into(),
                "roots" => tab.log_from = Some(vec!["other-root".into()]),
                "order" => tab.log_order.first_parent = !tab.log_order.first_parent,
                "query" => tab.search.query = "other".into(),
                "mode" => tab.search.code = !tab.search.code,
                "namespace" => {
                    let mut settings = crate::settings::Settings::get(&ctx);
                    settings.history_filter.namespaces = "refs/other/*".into();
                    settings.put_for_test(&ctx);
                }
                "connection" => {
                    crate::connections::Context {
                        error: Some("other connection".into()),
                        ..Default::default()
                    }
                    .run(|| answer(&mut tab, &ctx, &request, result));
                    assert!(tab.search.results.is_empty());
                    assert!(tab.search.problem.is_none());
                    assert!(!tab.search.running);
                    continue;
                }
                _ => {}
            }
            answer(&mut tab, &ctx, &request, result);
            assert!(tab.search.results.is_empty(), "{changed}");
            assert!(tab.search.problem.is_none(), "{changed}");
            assert!(tab.pending_goto.is_none(), "{changed}");
            assert!(!tab.search.running, "{changed}");
        }
    }
    Ok(())
}

#[test]
fn retirement_and_drop_cancel_only_the_search_owned_child() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let request = start(&mut tab, &ctx, "same")?;
    let sibling = tab.tx.cancel.child();
    tab.search.retire();
    assert!(request.cancel.is_cancelled());
    assert!(!tab.tx.cancel.is_cancelled());
    assert!(!sibling.is_cancelled());
    let request = start(&mut tab, &ctx, "same")?;
    drop(std::mem::take(&mut tab.search));
    assert!(request.cancel.is_cancelled());
    assert!(!tab.tx.cancel.is_cancelled());
    assert!(!sibling.is_cancelled());
    Ok(())
}

#[test]
fn clearing_or_invalidating_the_query_retires_inflight_work() -> Result<(), String> {
    for query in ["", "author:"] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx);
        let request = start(&mut tab, &ctx, "same")?;
        tab.search.query = query.into();
        let (_, jobs) = super::super::messages::record_jobs(|| tab.run_search(&ctx));
        assert!(jobs.is_empty());
        assert!(!tab.search.running);
        assert!(request.cancel.is_cancelled());
        answer(&mut tab, &ctx, &request, Ok(vec!["old".into()]));
        assert!(tab.search.results.is_empty());
        assert!(tab.pending_goto.is_none());
    }
    Ok(())
}
