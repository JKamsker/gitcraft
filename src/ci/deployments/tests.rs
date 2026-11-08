use std::collections::HashMap;

use super::*;

fn project(forge: Forge) -> Project {
    Project {
        forge,
        host: "github.com".to_owned(),
        path: "o/r".to_owned(),
    }
}

const DEPLOYMENTS: &str = r#"[
    {"id":30,"sha":"cccccccccccccccccccccccccccccccccccccccc","ref":"main","task":"deploy",
     "environment":"production","created_at":"2026-11-05T10:00:00Z",
     "updated_at":"2026-11-05T10:00:00Z"},
    {"id":20,"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main","task":"deploy",
     "environment":"production","created_at":"2026-11-04T10:00:00Z",
     "updated_at":"2026-11-04T10:00:00Z"},
    {"id":15,"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main","task":"deploy",
     "environment":"staging","created_at":"2026-11-04T09:00:00Z",
     "updated_at":"2026-11-04T09:00:00Z"},
    {"id":10,"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"main","task":"deploy",
     "environment":"production","created_at":"2026-11-01T10:00:00Z",
     "updated_at":"2026-11-01T10:00:00Z"}
]"#;

/// Answers `gh api` from recorded samples; what is asked is kept.
fn answers(samples: &[(&str, &str)]) -> (HashMap<String, String>, Vec<String>) {
    (
        samples
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        Vec::new(),
    )
}

#[test]
fn github_statuses_are_read_back_to_each_environments_latest_success() -> Result<(), String> {
    let (samples, mut asked) = answers(&[
        ("repos/o/r/deployments?per_page=50", DEPLOYMENTS),
        (
            "repos/o/r/environments?per_page=100",
            r#"{"total_count":3,"environments":[
                {"id":1,"name":"staging","html_url":"https://github.com/o/r/deployments/staging"},
                {"id":2,"name":"production","html_url":"https://github.com/o/r/deployments/production"},
                {"id":3,"name":"preview","html_url":""}]}"#,
        ),
        (
            "repos/o/r/deployments/30/statuses?per_page=1",
            r#"[{"state":"in_progress","created_at":"2026-11-05T10:00:05Z",
                 "updated_at":"2026-11-05T10:01:00Z",
                 "log_url":"https://github.com/o/r/actions/runs/777/job/8","target_url":"",
                 "environment_url":""}]"#,
        ),
        (
            "repos/o/r/deployments/20/statuses?per_page=1",
            r#"[{"state":"success","created_at":"2026-11-04T10:05:00Z",
                 "updated_at":"2026-11-04T10:06:00Z","log_url":"",
                 "target_url":"https://ci.example.org/deploy/20",
                 "environment_url":"https://app.example.org"}]"#,
        ),
        ("repos/o/r/deployments/15/statuses?per_page=1", "[]"),
    ]);
    let read = read_github(&project(Forge::GitHub), |rest| {
        asked.push(rest.to_owned());
        samples
            .get(rest)
            .cloned()
            .ok_or_else(|| format!("unexpected {rest}"))
    })?;
    // Deployment 10 is older than production's success: its status isn't asked.
    assert!(!asked.iter().any(|a| a.contains("deployments/10/")));
    let by_id = |id: u64| read.deployments.iter().find(|d| d.id == id);
    let rolling = by_id(30).ok_or("30")?;
    assert_eq!(rolling.state(), DeployState::InProgress);
    assert_eq!(rolling.run, Some(777));
    let live = by_id(20).ok_or("20")?;
    assert_eq!(live.state(), DeployState::Success);
    assert_eq!(live.reported, time("2026-11-04T10:06:00Z"));
    assert_eq!(live.url, "https://ci.example.org/deploy/20");
    assert_eq!(live.environment_url, "https://app.example.org");
    assert_eq!(by_id(15).map(|d| d.state()), Some(DeployState::NoStatus));
    assert_eq!(by_id(10).map(|d| d.state()), Some(DeployState::NotRead));
    let envs: Vec<(&str, &str)> = read
        .environments
        .iter()
        .map(|e| (e.name.as_str(), e.url.as_str()))
        .collect();
    assert_eq!(
        envs[2],
        ("preview", "https://github.com/o/r/deployments/preview")
    );
    assert!(read.partial.is_empty());
    Ok(())
}

#[test]
fn environments_that_cant_be_listed_make_it_partial_not_failed() -> Result<(), String> {
    let read = read_github(&project(Forge::GitHub), |rest| {
        if rest.contains("environments") {
            Err("HTTP 403".to_owned())
        } else if rest.contains("statuses") {
            Ok("[]".to_owned())
        } else {
            Ok(DEPLOYMENTS.to_owned())
        }
    })?;
    assert_eq!(read.deployments.len(), 4);
    assert!(read.partial[0].contains("HTTP 403"));
    assert!(read_github(&project(Forge::GitHub), |_| Ok("oops".to_owned())).is_err());
    Ok(())
}

#[test]
fn reads_gitlab_deployments_and_environments() -> Result<(), String> {
    let gl = Project {
        forge: Forge::GitLab,
        host: "gitlab.com".to_owned(),
        path: "g/p".to_owned(),
    };
    let read = read_gitlab(&gl, |rest| {
        Ok(if rest.starts_with("deployments") {
            r#"[{"id":41,"iid":3,"ref":"main","sha":"dddddddddddddddddddddddddddddddddddddddd",
                 "created_at":"2026-11-03T08:00:00.000Z","updated_at":"2026-11-03T08:04:00.000Z",
                 "status":"success",
                 "environment":{"id":5,"name":"staging","external_url":"https://staging.example"},
                 "deployable":{"id":99,"web_url":"https://gitlab.com/g/p/-/jobs/99",
                               "pipeline":{"id":321}}},
                {"id":40,"ref":"main","sha":"eeee","status":"blocked",
                 "created_at":"2026-11-03T07:00:00Z","updated_at":"2026-11-03T07:00:00Z",
                 "environment":{"id":6,"name":"production"},"deployable":null}]"#
        } else {
            r#"[{"id":5,"name":"staging"},{"id":6,"name":"production"},{"id":7,"name":"review/x"}]"#
        }
        .to_owned())
    })?;
    let d = &read.deployments[0];
    assert_eq!(
        (d.environment.as_str(), d.state(), d.run),
        ("staging", DeployState::Success, Some(321))
    );
    assert_eq!(d.url, "https://gitlab.com/g/p/-/jobs/99");
    assert_eq!(read.deployments[1].state(), DeployState::Queued);
    assert_eq!(
        read.deployments[1].url,
        "https://gitlab.com/g/p/-/environments/6"
    );
    assert_eq!(read.environments.len(), 3);
    Ok(())
}

#[test]
fn artifacts_keep_their_digest_when_the_forge_records_one() -> Result<(), String> {
    let json = r#"{"total_count":2,"artifacts":[
        {"id":5,"name":"dist","size_in_bytes":2048,"expired":false,
         "digest":"sha256:0123abcd","workflow_run":{"id":777}},
        {"id":6,"name":"logs","size_in_bytes":10,"expired":true,"digest":null}]}"#;
    let list = parse_github_artifacts(json, &project(Forge::GitHub), 777)?;
    assert_eq!(list[0].digest.as_deref(), Some("sha256:0123abcd"));
    assert_eq!(
        list[0].url,
        "https://github.com/o/r/actions/runs/777/artifacts/5"
    );
    assert_eq!(list[1].digest, None);
    assert!(list[1].expired);
    let jobs = r#"[{"id":9,"name":"build","web_url":"https://gitlab.com/g/p/-/jobs/9",
                    "artifacts_file":{"filename":"artifacts.zip","size":100}},
                   {"id":8,"name":"lint","web_url":"u"}]"#;
    let list = parse_gitlab_artifacts(jobs, 3)?;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "build: artifacts.zip");
    assert_eq!(list[0].digest, None);
    Ok(())
}

#[test]
fn states_map_from_either_forge() {
    assert_eq!(github_state("queued"), DeployState::Queued);
    assert_eq!(github_state("error"), DeployState::Failed);
    assert_eq!(github_state("inactive"), DeployState::Inactive);
    assert_eq!(gitlab_state("running"), DeployState::InProgress);
    assert_eq!(gitlab_state("canceled"), DeployState::Cancelled);
    assert!(DeployState::Queued.in_flight() && !DeployState::Success.in_flight());
    assert!(!DeployState::NoStatus.in_flight());
}

#[test]
fn forges_without_deployments_say_so() {
    let err = read(Path::new("/nonexistent"), &project(Forge::Forgejo)).err();
    assert!(err.is_some_and(|e| e.contains("Forgejo")));
}
