use super::*;
#[test]
fn hosted_and_legacy_remote_locations_are_canonical() -> Result<(), String> {
    for u in [
        "https://dev.azure.com/team/My%20Project/_git/repo",
        "git@ssh.dev.azure.com:v3/team/My%20Project/repo",
        "https://team.visualstudio.com/DefaultCollection/My%20Project/_git/repo",
    ] {
        assert_eq!(
            location(u),
            Some(("dev.azure.com".into(), "team/My Project/_git/repo".into()))
        );
    }
    assert!(location("http://dev.azure.com/team/project/_git/repo").is_none());
    assert!(location("https://dev.azure.com.evil/team/project/_git/repo").is_none());
    Ok(())
}

use crate::pr_post::{LineComment, Target, Verdict};
use crate::pull_request::DiffSide;
use std::sync::{Arc, Mutex};
fn hosting() -> Hosting {
    Hosting {
        forge: Forge::Azure,
        host: "dev.azure.com".into(),
        web: "https://dev.azure.com".into(),
        path: "team/My Project/_git/repo".into(),
    }
}
fn pr() -> Value {
    json!({"pullRequestId":7,"title":"Native review","description":"Description","status":"active","sourceRefName":"refs/heads/topic","targetRefName":"refs/heads/main","lastMergeSourceCommit":{"commitId":"a".repeat(40)},"lastMergeTargetCommit":{"commitId":"b".repeat(40)},"repository":{"id":"repo-id","project":{"id":"project-id"}},"createdBy":{"id":"author-id","displayName":"Author"},"reviewers":[{"id":"reviewer-id","displayName":"Reviewer","vote":10}],"mergeStatus":"succeeded"})
}
fn iteration() -> Value {
    json!({"id":2,"sourceRefCommit":{"commitId":"a".repeat(40)},"targetRefCommit":{"commitId":"b".repeat(40)},"commonRefCommit":{"commitId":"c".repeat(40)}})
}
#[derive(Default)]
struct Model {
    pr: Value,
    iteration: Value,
    calls: Vec<(String, String, Option<Value>)>,
    thread_error: bool,
    drift: bool,
    statuses: Option<Value>,
}
fn recording() -> (Api, Arc<Mutex<Model>>) {
    let model = Arc::new(Mutex::new(Model {
        pr: pr(),
        iteration: iteration(),
        ..Default::default()
    }));
    let state = model.clone();
    let api = Api::recording(
        Service::Azure,
        hosting(),
        "viewer-id",
        Arc::new(move |method, path, body| {
            let mut m = state.lock().map_err(|e| e.to_string())?;
            m.calls.push((method.into(), path.into(), body.cloned()));
            let p = path.split('?').next().ok_or("missing recording path")?;
            if method != "GET" {
                return Ok(m.pr.clone());
            }
            Ok(if p == "_apis/connectionData" {
                json!({"authenticatedUser":{"id":"viewer-id"}})
            } else if p == "My%20Project/_apis/git/repositories/repo" {
                json!({"id":"repo-id","name":"repo","project":{"id":"project-id","name":"My Project"}})
            } else if p.ends_with("/iterations") {
                json!({"value":[m.iteration.clone()]})
            } else if p.ends_with("/iterations/2/changes") {
                assert!(path.contains("$compareTo=0"));
                json!({"changeEntries":[{"changeTrackingId":42,"item":{"path":"/new.rs"},"originalPath":"/old.rs"}]})
            } else if p.ends_with("/items") {
                json!({"content":"first\nsecond\nthird\n"})
            } else if p.ends_with("/threads") {
                if m.thread_error {
                    return Err("recorded discussion failure".into());
                }
                json!({"value":[{"id":2,"status":"active","threadContext":{"filePath":"/old.rs","leftFileStart":{"line":2}},"pullRequestThreadContext":{"iterationContext":{"firstComparingIteration":1,"secondComparingIteration":1}},"comments":[{"id":1,"content":"Old discussion","author":{"displayName":"Reviewer"}}]}]})
            } else if p.ends_with("/threads/2") {
                json!({"id":2,"status":"active"})
            } else if p.ends_with("/commits") {
                json!({"value":[{"commitId":"a".repeat(40),"comment":"Change","author":{"name":"Author"}}]})
            } else if p.ends_with("/statuses") {
                if m.drift {
                    m.drift = false;
                    m.pr["lastMergeSourceCommit"]["commitId"] = json!("d".repeat(40));
                    m.iteration["sourceRefCommit"]["commitId"] = json!("d".repeat(40));
                    m.iteration["id"] = json!(3);
                }
                m.statuses.clone().unwrap_or_else(
                    || json!({"value":[{"context":{"name":"Build"},"state":"succeeded"}]}),
                )
            } else if p.ends_with("/policy/evaluations") {
                json!({"value":[{"status":"rejected","configuration":{"isBlocking":true,"type":{"displayName":"Required review"}}}]})
            } else if p.ends_with("/pullrequests/7") {
                m.pr.clone()
            } else if p.ends_with("/pullrequests") {
                json!({"value":[m.pr.clone()]})
            } else {
                return Err(format!("Unexpected recording route {method} {path}"));
            })
        }),
    );
    (api, model)
}
fn writes(m: &Model) -> Vec<&(String, String, Option<Value>)> {
    m.calls
        .iter()
        .filter(|(method, _, _)| method != "GET")
        .collect()
}
#[test]
fn native_page_reads_account_iteration_reviews_policies_and_outdated_threads() -> Result<(), String>
{
    let (api, _) = recording();
    let d = details::read(&api, 7)?;
    assert_eq!(d.forge, Some(Forge::Azure));
    assert_eq!(d.head_oid, "a".repeat(40));
    assert_eq!(d.base_oid, "c".repeat(40));
    assert_eq!(d.base_branch, "main");
    assert!(d.url.contains("My%20Project/_git/repo/pullrequest/7"));
    assert!(d.extras.native_policy && d.extras.merge_handoff);
    assert!(!d.extras.review_features.pending);
    assert!(d.extras.review_features.approve);
    assert!(d.extras.threads[0].outdated && d.extras.threads[0].can_resolve);
    assert_eq!(d.checks.len(), 2);
    assert_eq!(d.checks[1].state, crate::pull_request::CheckState::Failed);
    assert!(d.timeline.iter().any(|e| e.body == "Old discussion"));
    Ok(())
}
#[test]
fn native_writes_bind_changes_to_common_base_iteration_and_account() -> Result<(), String> {
    let (api, state) = recording();
    let d = details::read(&api, 7)?;
    let t = post::AzureTarget::new(api, &d).ok_or("missing Azure target")?;
    t.comment("Conversation")?;
    t.review(Verdict::Approve, "")?;
    t.review(Verdict::RequestChanges, "")?;
    t.comment_now(&LineComment {
        path: "new.rs".into(),
        line: 2,
        side: DiffSide::Left,
        body: "Old side".into(),
    })?;
    t.comment_now(&LineComment {
        path: "new.rs".into(),
        line: 3,
        side: DiffSide::Right,
        body: "New side".into(),
    })?;
    t.reply("2", "Reply to original discussion")?;
    t.set_resolved("2", true)?;
    t.set_resolved("2", false)?;
    let m = state.lock().map_err(|e| e.to_string())?;
    let w = writes(&m);
    assert_eq!(w.len(), 8);
    let positioned = w
        .iter()
        .filter_map(|(_, _, v)| v.as_ref())
        .filter(|v| v.get("threadContext").is_some())
        .collect::<Vec<_>>();
    assert_eq!(positioned.len(), 2);
    for v in &positioned {
        assert_eq!(v["pullRequestThreadContext"]["changeTrackingId"], 42);
        assert_eq!(
            v["pullRequestThreadContext"]["iterationContext"],
            json!({"firstComparingIteration":2,"secondComparingIteration":2})
        );
        assert_eq!(v["threadContext"]["filePath"], "/new.rs");
    }
    assert_eq!(positioned[0]["threadContext"]["leftFileStart"]["line"], 2);
    assert_eq!(positioned[1]["threadContext"]["rightFileStart"]["line"], 3);
    assert!(
        m.calls
            .iter()
            .any(|(_, p, _)| p.contains("path=%2Fold.rs") && p.contains(&"c".repeat(40)))
    );
    assert!(w.iter().any(|(_, p, b)| p.contains("reviewers/viewer-id")
        && b.as_ref().is_some_and(|v| v["vote"] == 10)));
    Ok(())
}
#[test]
fn stale_head_target_iteration_and_account_refuse_native_writes() -> Result<(), String> {
    for field in ["source", "target", "iteration", "repository"] {
        let (api, state) = recording();
        let d = details::read(&api, 7)?;
        let t = post::AzureTarget::new(api, &d).ok_or("missing Azure target")?;
        {
            let mut m = state.lock().map_err(|e| e.to_string())?;
            match field {
                "source" => {
                    m.pr["lastMergeSourceCommit"]["commitId"] = json!("e".repeat(40));
                    m.iteration["sourceRefCommit"]["commitId"] = json!("e".repeat(40));
                }
                "target" => {
                    m.pr["lastMergeTargetCommit"]["commitId"] = json!("e".repeat(40));
                    m.iteration["targetRefCommit"]["commitId"] = json!("e".repeat(40));
                }
                "iteration" => m.iteration["id"] = json!(3),
                _ => m.pr["repository"]["id"] = json!("other"),
            }
        }
        assert!(t.comment("Must refuse").is_err(), "{field}");
        assert!(writes(&*state.lock().map_err(|e| e.to_string())?).is_empty());
    }
    let (api, state) = recording();
    let d = details::read(&api, 7)?;
    let t = post::AzureTarget::new(api, &d).ok_or("missing Azure target")?;
    let mut changed = crate::connections::current();
    changed.error = Some("another profile".into());
    changed.run(|| assert!(t.reply("2", "No").is_err()));
    assert!(t.reply("999", "Not displayed").is_err());
    assert!(writes(&*state.lock().map_err(|e| e.to_string())?).is_empty());
    Ok(())
}
#[test]
fn source_read_drift_retries_and_partial_discussions_remain_explicit() -> Result<(), String> {
    let (api, state) = recording();
    state.lock().map_err(|e| e.to_string())?.drift = true;
    let d = details::read(&api, 7)?;
    assert_eq!(d.head_oid, "d".repeat(40));
    let (api, state) = recording();
    state.lock().map_err(|e| e.to_string())?.thread_error = true;
    let d = details::read(&api, 7)?;
    assert!(
        d.inline_error
            .as_deref()
            .ok_or("missing inline error")?
            .contains("discussion failure")
    );
    assert!(d.extras.threads.is_empty());
    Ok(())
}
#[test]
fn native_list_create_and_link_routes_are_reachable_without_external_writes() -> Result<(), String>
{
    let (api, state) = recording();
    let repo = repository(&api)?;
    let rows = list(&api, &repo, "searchCriteria.status=active")?;
    assert_eq!(parse(&rows[0], &hosting())?.branch, "topic");
    let n = NewPullRequest {
        branch: "topic",
        base: "main",
        title: "Native",
        body: "Description",
        draft: true,
        reviewers: &["reviewer-id".into()],
    };
    assert_eq!(create(&api, &n)?, pr_url(&hosting(), 7));
    let m = state.lock().map_err(|e| e.to_string())?;
    let w = writes(&m);
    assert_eq!(w.len(), 1);
    assert!(
        w[0].1
            .contains("project-id/_apis/git/repositories/repo-id/pullrequests?api-version=7.1")
    );
    assert_eq!(
        w[0].2.as_ref().ok_or("missing create body")?["sourceRefName"],
        "refs/heads/topic"
    );
    assert_eq!(
        w[0].2.as_ref().ok_or("missing create body")?["reviewers"][0]["id"],
        "reviewer-id"
    );
    assert_eq!(
        crate::pull_request::repo_of_url(&pr_url(&hosting(), 7)),
        Some(("dev.azure.com".into(), hosting().path))
    );
    assert!(Azure.pages().is_some());
    Ok(())
}

#[test]
fn historical_iteration_target_opens_and_current_pr_target_is_independently_bound()
-> Result<(), String> {
    let (api, state) = recording();
    let old = details::read(&api, 7)?;
    state.lock().map_err(|e| e.to_string())?.pr["lastMergeTargetCommit"]["commitId"] =
        json!("d".repeat(40));
    let stale = post::AzureTarget::new(api.clone(), &old).ok_or("old target")?;
    assert!(
        stale
            .comment("Do not publish against previous displayed target")
            .is_err()
    );
    assert!(writes(&*state.lock().map_err(|e| e.to_string())?).is_empty());
    let shown = details::read(&api, 7)?;
    assert_eq!(shown.head_oid, "a".repeat(40));
    assert_eq!(shown.base_oid, "c".repeat(40));
    let revision: Value =
        serde_json::from_str(shown.extras.review_revision.as_deref().ok_or("revision")?)
            .map_err(|e| e.to_string())?;
    assert_eq!(revision["target"]["commitId"], "d".repeat(40));
    assert_eq!(revision["iterationTarget"]["commitId"], "b".repeat(40));
    let current = post::AzureTarget::new(api, &shown).ok_or("current target")?;
    current.comment("Review stable current PR with historical iteration target")?;
    state.lock().map_err(|e| e.to_string())?.iteration["targetRefCommit"]["commitId"] =
        json!("e".repeat(40));
    assert!(current.comment("Iteration target also changed").is_err());
    assert_eq!(writes(&*state.lock().map_err(|e| e.to_string())?).len(), 1);
    Ok(())
}

#[test]
fn escaped_and_legacy_pr_urls_match_the_canonical_repository() {
    let h = hosting();
    let generated = pr_url(&h, 7);
    assert!(h.matches_pull_request(&generated));
    for remote in [
        "https://dev.azure.com/team/My%20Project/_git/repo",
        "https://team.visualstudio.com/DefaultCollection/My%20Project/_git/repo",
        "https://team.visualstudio.com/My%20Project/_git/repo",
        "https://dev.azure.com:443/team/My%20Project/_git/repo",
        "git@ssh.dev.azure.com:v3/team/My%20Project/repo",
    ] {
        assert!(h.matches_git_remote(remote), "{remote}");
        let remotes = [crate::git::Remote {
            name: "origin".into(),
            url: remote.into(),
        }];
        assert_eq!(
            crate::pull_request::remote_for(&remotes, &generated, &h),
            Some("origin")
        );
    }
    for wrong in [
        "https://ssh.dev.azure.com/v3/team/My%20Project/repo",
        "https://team.visualstudio.com/v3/team/My%20Project/repo",
        "https://dev.azure.com@evil.invalid/team/My%20Project/_git/repo",
        "https://evil.invalid@dev.azure.com/team/My%20Project/_git/repo",
        "https://user:password@team.visualstudio.com/My%20Project/_git/repo",
        "https://team.visualstudio.com:8443/My%20Project/_git/repo",
        "https://team.visualstudio.com/My%20Project/_git/repo?other=1",
        "https://team.visualstudio.com/My%20Project/_git/repo#fragment",
        "http://dev.azure.com/team/My%20Project/_git/repo",
        "https://dev.azure.com:8443/team/My%20Project/_git/repo",
        "https://dev.azure.com/team/My%20Project/_git/repo?other=1",
        "https://dev.azure.com/team/My%20Project/_git/repo#fragment",
        "https://dev.azure.com/team/my%20Project/_git/repo",
        "https://dev.azure.com/team/My%2fProject/_git/repo",
        "https://other.visualstudio.com/My%20Project/_git/repo",
        "https://team.visualstudio.com.evil/My%20Project/_git/repo",
    ] {
        assert!(!h.matches_http_repository(wrong), "{wrong}");
    }
}

#[test]
fn azure_link_query_values_round_trip_without_extra_parameters() -> Result<(), String> {
    use crate::forge::links::{Place, url};
    for branch in ["topic&foo=bar", "topic+name", "feature/naïve%20"] {
        let link = url(&hosting(), &Place::Branch(branch));
        let parsed = ::url::Url::parse(&link).map_err(|e| e.to_string())?;
        let pairs: Vec<_> = parsed.query_pairs().collect();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "version");
        assert_eq!(pairs[0].1, format!("GB{branch}"));
    }
    let path = "dir/a&version=GBmain+é%.rs";
    let link = url(
        &hosting(),
        &Place::File {
            sha: "abc123",
            path,
            lines: None,
        },
    );
    let parsed = ::url::Url::parse(&link).map_err(|e| e.to_string())?;
    let pairs: Vec<_> = parsed.query_pairs().collect();
    assert_eq!(pairs.len(), 3);
    assert_eq!(pairs[0].0, "path");
    assert_eq!(pairs[0].1, format!("/{path}"));
    assert_eq!(pairs[1], ("version".into(), "GCabc123".into()));
    assert_eq!(pairs[2], ("_a".into(), "contents".into()));
    Ok(())
}

#[test]
fn incomplete_commit_reads_surface_checks_errors_including_clamped_pages() -> Result<(), String> {
    for (count, continuation) in [
        (1, Some("next-page")),
        (999, Some("next-page")),
        (1000, None),
    ] {
        let (mut api, _) = recording();
        let record = api.record.clone().ok_or("missing recorder")?;
        api.record = Some(Arc::new(move |method, path, body, accept| {
            if path
                .split('?')
                .next()
                .is_some_and(|p| p.ends_with("/commits"))
            {
                let rows = vec![json!({"commitId":"a".repeat(40),"comment":"Change"}); count];
                let body = json!({"value":rows}).to_string();
                let header = continuation
                    .map(|t| format!("X-MS-ContinuationToken: {t}\r\n"))
                    .unwrap_or_default();
                // Feed an actual recorded HTTP header through the production reader.
                return format!(
                    "HTTP/1.1 200 OK\r\n{header}Content-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .parse::<ureq::Response>()
                .map_err(|e| e.to_string());
            }
            record(method, path, body, accept)
        }));
        let d = details::read(&api, 7)?;
        assert!(d.commits.is_empty());
        let error = d
            .checks_error
            .as_deref()
            .ok_or("incomplete read was silent")?;
        assert!(error.contains("incomplete"), "{error}");
        assert!(error.contains("forge page"), "{error}");
        assert!(!d.checks.is_empty());
    }
    let (api, _) = recording();
    let d = details::read(&api, 7)?;
    assert_eq!(d.commits.len(), 1);
    assert!(d.checks_error.is_none());
    Ok(())
}

#[test]
fn checks_keep_only_global_and_current_iteration_with_distinct_contexts() -> Result<(), String> {
    use crate::pull_request::CheckState;
    let (api, state) = recording();
    state.lock().map_err(|e| e.to_string())?.statuses = Some(json!({"value":[
        {"iterationId":1,"context":{"genre":"ci","name":"Build"},"state":"failed","description":"old failure"},
        {"iterationId":1,"context":{"genre":"security","name":"Build"},"state":"succeeded","description":"old success"},
        {"context":{"genre":"global","name":"Build"},"state":"pending","description":"global status"},
        {"iterationId":2,"context":{"genre":"ci","name":"Build"},"state":"succeeded","description":"current success"},
        {"iterationId":2,"context":{"genre":"security","name":"Build"},"state":"failed","description":"current failure"},
        {"iterationId":3,"context":{"name":"Future"},"state":"succeeded"},
        {"iterationId":null,"context":{"name":"Unknown"},"state":"succeeded"}
    ]}));
    let d = details::read(&api, 7)?;
    assert!(d.checks_error.is_none());
    assert_eq!(d.checks.len(), 4); // Three relevant statuses plus the policy.
    assert_eq!(d.checks[0].name, "global / Build");
    assert_eq!(d.checks[0].state, CheckState::Running);
    assert_eq!(d.checks[1].name, "ci / Build");
    assert_eq!(d.checks[1].state, CheckState::Passed);
    assert_eq!(d.checks[2].name, "security / Build");
    assert_eq!(d.checks[2].state, CheckState::Failed);
    assert!(d.checks.iter().all(|c| !c.detail.starts_with("old ")));
    Ok(())
}
