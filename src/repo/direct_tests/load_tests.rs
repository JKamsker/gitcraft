/// The pull requests read before a merge answer after those read since: the merged one
/// doesn't come back.
#[test]
fn an_older_list_of_pull_requests_answering_last_is_dropped() {
    use std::time::{Duration, Instant};

    use super::super::{Msg, RepoTab};
    use crate::forge::PullRequest;

    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/prs-order-test".into(), &ctx);
    let merged = PullRequest {
        number: 7,
        title: "merged since".to_owned(),
        branch: "feat".to_owned(),
        base: "main".to_owned(),
        base_oid: String::new(),
        head_repo: None,
        from_fork: false,
        sha: String::new(),
        author: String::new(),
        url: String::new(),
        draft: false,
    };
    let before = Instant::now();
    let since = before + Duration::from_millis(1);
    tab.prs_loaded = Some(since);
    for msg in [
        Msg::Prs(since, Ok(Vec::new())),
        Msg::Prs(before, Ok(vec![merged])),
    ] {
        let _ = tab.tx.send(msg);
    }
    tab.poll(&ctx);
    assert!(tab.prs.is_empty());
}

#[test]
fn explicit_refresh_retires_recent_request_and_preserves_display_on_failure() {
    use super::super::{Msg, RepoTab};
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/prs-refresh-test".into(), &ctx);
    let recent = std::time::Instant::now();
    tab.prs_loaded = Some(recent);
    // A running repository read queues another reload without spawning work in this fixture.
    tab.life.phase = crate::repo::lifecycle::Phase::Live;
    tab.loading = true;
    tab.refresh_on_focus(&ctx);
    assert_eq!(
        tab.prs_loaded,
        Some(recent),
        "focus keeps background freshness"
    );
    tab.refresh(&ctx);
    assert!(
        tab.prs_loaded.is_none(),
        "refresh within 120s must bypass freshness"
    );
    tab.handle(Msg::Prs(recent, Err("retired request".into())), &ctx);
    assert!(tab.pr_error.is_none(), "retired response must be ignored");
    let current = std::time::Instant::now();
    let displayed: crate::forge::PullRequest = serde_json::from_value(serde_json::json!({
        "number": 7, "title": "previous complete list", "branch": "feature", "base": "main",
        "from_fork": true, "sha": "head", "author": "contributor", "url": "url", "draft": true
    }))
    .unwrap_or_else(|e| unreachable!("fixture: {e}"));
    tab.prs = vec![displayed.clone()];
    tab.prs_loaded = Some(current);
    tab.handle(Msg::Prs(current, Err("page two failed".into())), &ctx);
    assert_eq!(tab.pr_error.as_deref(), Some("page two failed"));
    assert_eq!(tab.prs, vec![displayed]);
}
