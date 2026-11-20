//! The request sequence against a mocked transport: every quality-gate state, wrong branch,
//! access denied, timeout and version/edition differences.
use std::cell::RefCell;
use std::rc::Rc;

use super::super::gate::{Stale, Unavailable, Verdict, evaluate};
use super::super::{Failure, Provider, Query, Target};
use super::client::{Auth, Client};
use super::{Account, Edition};

const HEAD: &str = "be6c75b85da526349c44e3978374c95e0b80a96d";
const OLD: &str = "bfe36592eb7f9f2708b5d358b5b5f33ed535c8cf";

type Calls = Rc<RefCell<Vec<(String, Auth)>>>;

/// Answers by the first route whose prefix matches; unknown paths are a 404.
fn client(
    edition: Edition,
    routes: Vec<(&'static str, Result<String, Failure>)>,
) -> (
    Client<impl FnMut(&str, Auth) -> Result<String, Failure>>,
    Calls,
) {
    let calls: Calls = Rc::default();
    let seen = calls.clone();
    let account = Account {
        edition,
        origin: match edition {
            Edition::Server => "https://sonar.example.com/sonarqube/".into(),
            Edition::Cloud => "https://sonarcloud.io".into(),
        },
        organization: if edition == Edition::Cloud {
            "my-org".into()
        } else {
            String::new()
        },
        secret_ref: "profile-1".into(),
    };
    let send = move |path: &str, auth: Auth| {
        seen.borrow_mut().push((path.to_owned(), auth));
        routes
            .iter()
            .find(|(prefix, _)| path.starts_with(prefix))
            .map_or(Err(Failure::Http(404)), |(_, answer)| answer.clone())
    };
    (Client::new(account, send), calls)
}

fn ok(body: &str) -> Result<String, Failure> {
    Ok(body.to_owned())
}

fn branches() -> Result<String, Failure> {
    ok(
        r#"{"branches":[{"name":"main","isMain":true},{"name":"feature/x","isMain":false,"analysisDate":"2024-01-02T03:04:05+0000"}]}"#,
    )
}

fn analysis(revision: &str) -> Result<String, Failure> {
    Ok(format!(
        r#"{{"paging":{{"pageIndex":1,"pageSize":1,"total":1}},"analyses":[{{"key":"AX-1","date":"2024-01-02T03:04:05+0000","revision":"{revision}"}}]}}"#
    ))
}

fn status(gate: &str) -> Result<String, Failure> {
    Ok(format!(
        r#"{{"projectStatus":{{"status":"{gate}","conditions":[{{"status":"ERROR","metricKey":"new_coverage"}}]}}}}"#
    ))
}

fn server(
    version: &str,
    revision: &str,
    gate: &str,
    ce: &str,
) -> Vec<(&'static str, Result<String, Failure>)> {
    vec![
        ("api/server/version", Ok(version.to_owned())),
        ("api/project_branches/list", branches()),
        ("api/project_analyses/search", analysis(revision)),
        ("api/qualitygates/project_status", status(gate)),
        ("api/ce/component", Ok(ce.to_owned())),
    ]
}

fn branch(name: &str) -> Query {
    Query {
        project: "my:project".into(),
        target: Target::Branch(name.into()),
    }
}

#[test]
fn a_current_server_passes_with_bearer_and_binds_the_gate_to_the_analysis() -> Result<(), String> {
    let (mut c, calls) = client(Edition::Server, server("10.4.1.88267", HEAD, "OK", "{}"));
    let o = c
        .quality_gate(&branch("feature/x"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(evaluate(&o, Some(HEAD)), Verdict::Passed);
    assert_eq!(
        o.dashboard,
        "https://sonar.example.com/sonarqube/dashboard?id=my%3Aproject&branch=feature%2Fx"
    );
    let calls = calls.borrow();
    assert_eq!(calls[0], ("api/server/version".to_owned(), Auth::None));
    assert!(calls[1..].iter().all(|(_, auth)| *auth == Auth::Bearer));
    assert!(
        calls.iter().any(|(p, _)| p
            == "api/project_analyses/search?project=my%3Aproject&branch=feature%2Fx&ps=1")
    );
    assert!(
        calls
            .iter()
            .any(|(p, _)| p == "api/qualitygates/project_status?analysisId=AX-1")
    );
    Ok(())
}

#[test]
fn the_main_branch_is_asked_without_a_branch_parameter() -> Result<(), String> {
    let (mut c, calls) = client(
        Edition::Server,
        server("26.6.0.123539", HEAD, "ERROR", "{}"),
    );
    let o = c
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(evaluate(&o, Some(HEAD)), Verdict::Failed);
    assert_eq!(o.failing, ["new_coverage"]);
    assert!(o.dashboard.ends_with("dashboard?id=my%3Aproject"));
    assert!(
        calls
            .borrow()
            .iter()
            .any(|(p, _)| p == "api/project_analyses/search?project=my%3Aproject&ps=1")
    );
    Ok(())
}

#[test]
fn servers_before_10_use_basic_tokens_and_before_7_7_are_unsupported() -> Result<(), String> {
    let (mut c, calls) = client(Edition::Server, server("9.9.4.87374", HEAD, "OK", "{}"));
    c.quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert!(calls.borrow()[1..].iter().all(|(_, a)| *a == Auth::Basic));
    let (mut old, calls) = client(Edition::Server, server("7.6.0.21501", HEAD, "OK", "{}"));
    assert_eq!(
        old.quality_gate(&branch("main")),
        Err(Failure::UnsupportedVersion("7.6.0.21501".into()))
    );
    assert_eq!(
        calls.borrow().len(),
        1,
        "nothing is asked of an unsupported server"
    );
    // A server refusing even the version endpoint is assumed current.
    let mut routes = server("", HEAD, "OK", "{}");
    routes[0].1 = Err(Failure::Unauthorized);
    let (mut locked, calls) = client(Edition::Server, routes);
    locked
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(calls.borrow()[1].1, Auth::Bearer);
    Ok(())
}

#[test]
fn queued_tasks_make_it_pending_and_a_failed_newest_task_makes_it_stale() -> Result<(), String> {
    let queued = r#"{"queue":[{"status":"PENDING","branch":"feature/x","branchType":"BRANCH"}]}"#;
    let (mut c, _) = client(Edition::Server, server("10.0", HEAD, "OK", queued));
    let o = c
        .quality_gate(&branch("feature/x"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(evaluate(&o, Some(HEAD)), Verdict::Pending);
    // Another branch's queue does not concern this one.
    let (mut c, _) = client(Edition::Server, server("10.0", HEAD, "OK", queued));
    let o = c
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(evaluate(&o, Some(HEAD)), Verdict::Passed);
    // Community editions omit the branch of main's tasks.
    let failed = r#"{"queue":[],"current":{"status":"FAILED"}}"#;
    let (mut c, _) = client(Edition::Server, server("10.0", HEAD, "OK", failed));
    let o = c
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        evaluate(&o, Some(HEAD)),
        Verdict::Stale(Stale::TaskFailed, super::super::gate::ServerGate::Ok)
    );
    Ok(())
}

#[test]
fn another_commit_or_another_branch_is_never_shown_as_passed() -> Result<(), String> {
    let (mut c, _) = client(Edition::Server, server("10.0", OLD, "OK", "{}"));
    let o = c
        .quality_gate(&branch("feature/x"))
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(
        evaluate(&o, Some(HEAD)),
        Verdict::Stale(Stale::Mismatch { .. }, _)
    ));
    // feature/y was never analyzed: feature/x's or main's passing gate must not stand in.
    let (mut c, calls) = client(Edition::Server, server("10.0", HEAD, "OK", "{}"));
    let o = c
        .quality_gate(&branch("feature/y"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        evaluate(&o, Some(HEAD)),
        Verdict::Unavailable(Unavailable::NotAnalyzed)
    );
    assert!(
        !calls.borrow().iter().any(
            |(p, _)| p.starts_with("api/qualitygates") || p.starts_with("api/project_analyses")
        )
    );
    // A branch whose analyses were purged.
    let mut routes = server("10.0", HEAD, "OK", "{}");
    routes[2].1 = ok(r#"{"paging":{"pageIndex":1,"pageSize":1,"total":0},"analyses":[]}"#);
    let (mut c, _) = client(Edition::Server, routes);
    let o = c
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        evaluate(&o, Some(HEAD)),
        Verdict::Unavailable(Unavailable::NotAnalyzed)
    );
    // A gate status that vanished (404 for the analysis) is unrecognized, not passed.
    let mut routes = server("10.0", HEAD, "OK", "{}");
    routes[3].1 = Err(Failure::Http(404));
    let (mut c, _) = client(Edition::Server, routes);
    let o = c
        .quality_gate(&branch("main"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        evaluate(&o, Some(HEAD)),
        Verdict::Unavailable(Unavailable::Unrecognized)
    );
    Ok(())
}

fn cloud_pr(org: &str, sha: &str) -> Vec<(&'static str, Result<String, Failure>)> {
    vec![
        ("api/server/version", ok("8.0.0.118008")),
        (
            "api/components/show",
            Ok(format!(
                r#"{{"component":{{"organization":"{org}","key":"my:project"}}}}"#
            )),
        ),
        ("api/project_branches/list", branches()),
        (
            "api/project_pull_requests/list",
            Ok(format!(
                r#"{{"pullRequests":[{{"key":"42","branch":"feature/x","analysisDate":"2024-01-02T03:04:05+0000","commit":{{"sha":"{sha}"}}}}]}}"#
            )),
        ),
        ("api/qualitygates/project_status", status("OK")),
        ("api/ce/component", Err(Failure::Forbidden)),
    ]
}

fn pull_request(key: &str) -> Query {
    Query {
        project: "my:project".into(),
        target: Target::PullRequest(key.into()),
    }
}

#[test]
fn cloud_pull_requests_check_the_organization_and_the_head() -> Result<(), String> {
    let (mut c, calls) = client(Edition::Cloud, cloud_pr("MY-ORG", HEAD));
    let o = c
        .quality_gate(&pull_request("42"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(evaluate(&o, Some(HEAD)), Verdict::Passed);
    assert_eq!(
        o.dashboard,
        "https://sonarcloud.io/dashboard?id=my%3Aproject&pullRequest=42"
    );
    assert!(calls.borrow()[1..].iter().all(|(_, a)| *a == Auth::Bearer));
    assert!(calls.borrow().iter().any(
        |(p, _)| p == "api/qualitygates/project_status?projectKey=my%3Aproject&pullRequest=42"
    ));
    // Sonar often analyzes the merge commit: that is not the open head.
    let (mut c, _) = client(Edition::Cloud, cloud_pr("my-org", OLD));
    let o = c
        .quality_gate(&pull_request("42"))
        .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(evaluate(&o, Some(HEAD)), Verdict::Stale(..)));
    let (mut c, _) = client(Edition::Cloud, cloud_pr("my-org", HEAD));
    let o = c
        .quality_gate(&pull_request("43"))
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        evaluate(&o, Some(HEAD)),
        Verdict::Unavailable(Unavailable::NotAnalyzed)
    );
    let (mut c, calls) = client(Edition::Cloud, cloud_pr("someone-else", HEAD));
    assert_eq!(
        c.quality_gate(&pull_request("42")),
        Err(Failure::WrongOrganization)
    );
    assert_eq!(calls.borrow().len(), 2);
    Ok(())
}

#[test]
fn access_denied_missing_projects_timeouts_and_editions_fail_explicitly() {
    let cases = [
        (2, Err(Failure::Forbidden), Failure::Forbidden),
        (1, Err(Failure::Http(404)), Failure::ProjectNotFound),
        (1, Err(Failure::Unauthorized), Failure::Unauthorized),
        (3, Err(Failure::Timeout), Failure::Timeout),
        (
            2,
            Err(Failure::RateLimited(Some(30))),
            Failure::RateLimited(Some(30)),
        ),
        (0, Err(Failure::Unreachable), Failure::Unreachable),
    ];
    for (index, answer, expected) in cases {
        let mut routes = server("10.0", HEAD, "OK", "{}");
        routes[index].1 = answer;
        let (mut c, _) = client(Edition::Server, routes);
        assert_eq!(c.quality_gate(&branch("main")), Err(expected));
    }
    // An edition without pull request analysis answers "Unknown url".
    let (mut c, _) = client(Edition::Server, server("10.0", HEAD, "OK", "{}"));
    assert_eq!(
        c.quality_gate(&pull_request("42")),
        Err(Failure::PullRequestsUnsupported)
    );
    let (mut c, calls) = client(Edition::Server, server("10.0", HEAD, "OK", "{}"));
    let invalid = Query {
        project: "x&admin=1".into(),
        target: Target::Branch("main".into()),
    };
    assert_eq!(c.quality_gate(&invalid), Err(Failure::ProjectNotFound));
    assert!(calls.borrow().is_empty());
}

#[test]
fn connection_check_validates_the_token() {
    let mut routes = server("10.6.0.92116", HEAD, "OK", "{}");
    routes.push(("api/authentication/validate", ok(r#"{"valid":true}"#)));
    let (mut c, _) = client(Edition::Server, routes);
    assert_eq!(c.check().map(|c| c.version), Ok("10.6.0.92116".to_owned()));
    let mut routes = server("10.6", HEAD, "OK", "{}");
    routes.push(("api/authentication/validate", ok(r#"{"valid":false}"#)));
    let (mut c, _) = client(Edition::Server, routes);
    assert_eq!(c.check(), Err(Failure::Unauthorized));
}
