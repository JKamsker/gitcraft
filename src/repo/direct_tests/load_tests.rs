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
