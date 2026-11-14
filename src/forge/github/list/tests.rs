use super::*;
use crate::forge::{Forge, cli};

fn hosting() -> Hosting {
    Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        path: "owner/repo".into(),
        web: "https://github.com".into(),
    }
}
fn node(number: u64, incoming: bool) -> Value {
    json!({"number": number, "title": "Change", "headRefName": "feature",
        "baseRefName": "main", "baseRefOid": "base", "headRefOid": "head",
        "author": {"login": if incoming { "contributor" } else { "owner" }},
        "url": format!("https://github.com/owner/repo/pull/{number}"),
        "isDraft": incoming, "isCrossRepository": incoming,
        "headRepository": {"nameWithOwner": if incoming { "contributor/fork" } else { "owner/repo" }} })
}
fn page(nodes: Vec<Value>, next: bool, cursor: Value) -> String {
    json!({"data": {"repository": {"viewerPermission": "READ", "pullRequests": {
        "nodes": nodes, "pageInfo": {"hasNextPage": next, "endCursor": cursor}
    }}}})
    .to_string()
}
#[test]
fn incoming_and_owner_prs_survive_all_pages_with_read_access() -> Result<(), String> {
    let calls = cli::recording::run(
        vec![
            Ok(page(
                (1..=100).map(|n| node(n, false)).collect(),
                true,
                json!("next"),
            )),
            Ok(page(
                vec![node(100, false), node(101, true)],
                false,
                Value::Null,
            )),
        ],
        || {
            let prs = fetch(&std::env::temp_dir(), &hosting())?;
            assert_eq!(prs.len(), 101);
            let own = &prs[0];
            assert_eq!(
                (&*own.author, own.from_fork, own.draft),
                ("owner", false, false)
            );
            assert_eq!(own.head_repo.as_deref(), Some("owner/repo"));
            let incoming = &prs[100];
            assert_eq!(
                (&*incoming.author, incoming.from_fork, incoming.draft),
                ("contributor", true, true)
            );
            assert_eq!((&*incoming.branch, &*incoming.base), ("feature", "main"));
            assert_eq!(incoming.head_repo.as_deref(), Some("contributor/fork"));
            assert_eq!(incoming.url, "https://github.com/owner/repo/pull/101");
            Ok(())
        },
    )?;
    for (i, call) in calls.iter().enumerate() {
        let input: Value = serde_json::from_str(call.input.as_deref().ok_or("stdin")?)
            .map_err(|e| e.to_string())?;
        assert_eq!(
            input["variables"],
            json!({"owner": "owner", "name": "repo", "after": if i == 0 { Value::Null } else { json!("next") }})
        );
        assert_eq!(input["query"], QUERY);
        assert!(!QUERY.contains("viewer"));
        assert!(!QUERY.contains("headRefName:"));
        assert!(
            call.args
                .windows(2)
                .any(|p| p == ["--hostname", "github.com"])
        );
    }
    Ok(())
}
#[test]
fn empty_and_deleted_fork_and_old_cache() -> Result<(), String> {
    cli::recording::run(vec![Ok(page(vec![], false, Value::Null))], || {
        assert!(fetch(&std::env::temp_dir(), &hosting())?.is_empty());
        Ok(())
    })?;
    let mut deleted = node(7, true);
    deleted["headRepository"] = Value::Null;
    let prs: Vec<PullRequest> = parse(&page(vec![deleted], false, Value::Null))?
        .nodes
        .into_iter()
        .map(Into::into)
        .collect();
    assert_eq!(prs[0].head_repo, None);
    let mut cache = serde_json::to_value(&prs[0]).map_err(|e| e.to_string())?;
    cache.as_object_mut().ok_or("object")?.remove("head_repo");
    assert_eq!(
        serde_json::from_value::<PullRequest>(cache).map_err(|e| e.to_string())?,
        prs[0]
    );
    Ok(())
}
#[test]
fn partial_results_never_escape_failures_or_invalid_cursors() -> Result<(), String> {
    for failure in [
        Err("page two failed".into()),
        Ok("oops".into()),
        Ok("{}".into()),
        Ok("{\"errors\":[{\"message\":\"denied\"}]}".into()),
        Ok(page(vec![], true, Value::Null)),
        Ok(page(vec![], true, json!(""))),
        Ok(page(vec![], true, json!("next"))),
    ] {
        cli::recording::run(
            vec![Ok(page(vec![node(1, false)], true, json!("next"))), failure],
            || {
                assert!(fetch(&std::env::temp_dir(), &hosting()).is_err());
                Ok(())
            },
        )?;
    }
    for malformed in ["null", "[]", "{}", "{\"data\":{\"repository\":null}}"] {
        assert!(parse(malformed).is_err());
    }
    let mut malformed = node(1, false);
    malformed["isDraft"] = json!("invalid");
    assert!(parse(&page(vec![malformed], false, Value::Null)).is_err());
    Ok(())
}
