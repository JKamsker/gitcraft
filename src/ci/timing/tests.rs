use super::*;

const GITHUB_RUN: &str = r#"{"id":30433642,"name":"CI","head_branch":"feat/x y",
        "head_sha":"acb5820ced9479c074f688cc328bf03f341a511d","path":".github/workflows/ci.yml",
        "run_number":562,"event":"push","status":"completed","conclusion":"failure",
        "workflow_id":159038,"html_url":"https://github.com/o/r/actions/runs/30433642",
        "created_at":"2026-11-02T20:00:00Z","updated_at":"2026-11-02T20:10:00Z",
        "run_attempt":2,"run_started_at":"2026-11-02T20:00:05Z"}"#;

const GITHUB_JOBS: &str = r#"{"total_count":3,"jobs":[
        {"id":1,"run_id":30433642,"name":"lint","status":"completed","conclusion":"success",
         "created_at":"2026-11-02T20:00:06Z","started_at":"2026-11-02T20:00:16Z",
         "completed_at":"2026-11-02T20:01:16Z",
         "html_url":"https://github.com/o/r/actions/runs/30433642/job/1",
         "labels":["ubuntu-latest"],"runner_name":"GitHub Actions 2",
         "runner_group_name":"GitHub Actions",
         "steps":[{"name":"Set up job","status":"completed","conclusion":"success","number":1,
                   "started_at":"2026-11-02T20:00:16Z","completed_at":"2026-11-02T20:00:18Z"},
                  {"name":"Lint","status":"completed","conclusion":"success","number":2,
                   "started_at":"2026-11-02T20:00:18Z","completed_at":"2026-11-02T20:01:15Z"}]},
        {"id":2,"run_id":30433642,"name":"test (windows-latest)","status":"queued",
         "conclusion":null,"created_at":"2026-11-02T20:01:17Z","started_at":null,
         "completed_at":null,"html_url":"https://github.com/o/r/actions/runs/30433642/job/2",
         "labels":["windows-latest"],"runner_name":null,"runner_group_name":null,"steps":[]}
    ]}"#;

#[test]
fn reads_a_github_run_with_its_workflow_file() {
    let run = parse_github_run(GITHUB_RUN).unwrap_or_default();
    assert_eq!((run.id, run.number, run.attempt), (30_433_642, 562, 2));
    assert_eq!(run.workflow_id, Some(159_038));
    assert_eq!(run.path, ".github/workflows/ci.yml");
    assert_eq!(run.state, CheckState::Failed);
    assert_eq!(run.started.zip(run.created).map(|(s, c)| s - c), Some(5));
    assert_eq!(run.finished.zip(run.started).map(|(f, s)| f - s), Some(595));
    assert_eq!(workflow_path("dynamic/pages/pages-build-deployment"), "");
    assert_eq!(
        workflow_path(".github/workflows/reuse.yml@refs/heads/main"),
        ".github/workflows/reuse.yml"
    );
}

#[test]
fn reads_when_github_jobs_were_queued_and_where_they_ran() {
    let (jobs, more) = parse_github_jobs(GITHUB_JOBS).unwrap_or_default();
    assert!(more, "total_count says 3, 2 were read");
    assert_eq!(jobs.len(), 2);
    let lint = &jobs[0];
    assert_eq!(lint.started.zip(lint.created).map(|(s, c)| s - c), Some(10));
    assert_eq!(lint.ready(), lint.created);
    assert_eq!(
        lint.runner,
        [
            ("runner".to_owned(), "GitHub Actions 2".to_owned()),
            ("runner group".to_owned(), "GitHub Actions".to_owned()),
            ("labels".to_owned(), "ubuntu-latest".to_owned()),
        ]
    );
    assert_eq!(lint.steps.len(), 2);
    assert_eq!(
        lint.step_url(&lint.steps[1]),
        "https://github.com/o/r/actions/runs/30433642/job/1#step:2:1"
    );
    let test = &jobs[1];
    assert_eq!(test.state, CheckState::Running);
    assert_eq!((test.started, test.finished), (None, None));
    assert_eq!(
        test.runner,
        [("labels".to_owned(), "windows-latest".to_owned())]
    );
}

#[test]
fn reads_the_successful_runs_of_a_workflow() {
    let json = format!(r#"{{"total_count":1,"workflow_runs":[{GITHUB_RUN}]}}"#);
    let runs = parse_github_runs(&json).unwrap_or_default();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].id, 30_433_642);
    assert!(parse_github_runs("oops").is_err());
}

#[test]
fn reads_gitlab_queue_times_and_runners() {
    let pipeline = r#"{"id":48,"iid":13,"status":"success","source":"push","ref":"main",
            "sha":"a91957a858320c0e17f3a0eca7cfacbff50ea29a","name":null,
            "web_url":"https://gitlab.com/g/p/-/pipelines/48",
            "created_at":"2026-11-02T20:00:00.000Z","started_at":"2026-11-02T20:00:30.000Z",
            "finished_at":"2026-11-02T20:05:00.000Z","updated_at":"2026-11-02T20:06:00.000Z",
            "duration":270,"queued_duration":30}"#;
    let run = parse_gitlab_pipeline(pipeline).unwrap_or_default();
    assert_eq!(run.name, "Pipeline");
    assert_eq!(run.path, ".gitlab-ci.yml");
    assert_eq!(run.finished.zip(run.started).map(|(f, s)| f - s), Some(270));
    let jobs = r#"[
            {"id":9,"status":"failed","stage":"test","name":"rspec 1/2","web_url":"u9",
             "created_at":"2026-11-02T20:00:00.000Z","started_at":"2026-11-02T20:01:00.000Z",
             "finished_at":"2026-11-02T20:03:30.000Z","queued_duration":12.4,
             "runner":{"id":3,"description":"shared-runner-7","runner_type":"instance_type"},
             "tag_list":["docker","linux"]},
            {"id":8,"status":"success","stage":"build","name":"compile","web_url":"u8",
             "created_at":"2026-11-02T20:00:00.000Z","started_at":"2026-11-02T20:00:05.000Z",
             "finished_at":"2026-11-02T20:00:55.000Z","queued_duration":null,"runner":null,
             "tag_list":[]}
        ]"#;
    let jobs = parse_gitlab_jobs(jobs).unwrap_or_default();
    assert_eq!(jobs[0].name, "compile");
    assert_eq!(jobs[0].queued, None);
    assert!(jobs[0].runner.is_empty());
    let rspec = &jobs[1];
    assert_eq!(rspec.queued, Some(12));
    // It waited 12 s for a runner, not the minute since its pipeline was created.
    assert_eq!(rspec.ready(), rspec.started.map(|s| s - 12));
    assert_ne!(rspec.ready(), rspec.created);
    assert_eq!(jobs[0].ready(), jobs[0].created);
    assert_eq!(rspec.state, CheckState::Failed);
    assert_eq!(
        rspec.runner[0],
        ("runner".to_owned(), "shared-runner-7".to_owned())
    );
    assert_eq!(
        rspec.runner[2],
        ("tags".to_owned(), "docker, linux".to_owned())
    );
    assert!(parse_gitlab_jobs("oops").is_err());
}

#[test]
fn branches_are_escaped_in_queries() {
    assert_eq!(query_value("feat/x y&z"), "feat%2Fx%20y%26z");
    assert_eq!(query_value("main"), "main");
}

#[test]
fn forges_without_builds_say_so() {
    let project = Project {
        forge: Forge::Forgejo,
        host: "codeberg.org".to_owned(),
        path: "o/r".to_owned(),
    };
    let err = timed_run(Path::new("/nonexistent"), &project, 1).err();
    assert!(err.is_some_and(|e| e.contains("Forgejo")));
}
