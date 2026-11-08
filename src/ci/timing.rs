//! The timings of one run in more detail than [`super::jobs`] reads them: when each job was
//! queued as well as when it started and ended, what the forge recorded of the runner it ran
//! on, the workflow file the run came from, and the earlier successful runs of the same
//! workflow, to compare it with. Everything here only reads.

use std::path::Path;

use serde_json::Value;

use super::{CheckState, Project, github, gitlab, time};
use crate::forge::{self, Forge};
use crate::pull_request::{at, items};

/// So many jobs of a run are read; a run with more is read in part.
pub const MAX_JOBS: usize = 100;

/// One run (GitHub) or pipeline (GitLab), with what is needed to find its workflow file and
/// its earlier runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunInfo {
    pub id: u64,
    /// The workflow's name, or "Pipeline".
    pub name: String,
    pub number: u64,
    pub attempt: u64,
    /// The workflow's id (GitHub).
    pub workflow_id: Option<u64>,
    /// The workflow file at the run's commit: `.github/workflows/ci.yml`, `.gitlab-ci.yml`.
    /// Empty when the run didn't come from a file of the repository.
    pub path: String,
    pub branch: String,
    pub sha: String,
    pub event: String,
    pub state: CheckState,
    pub created: Option<i64>,
    pub started: Option<i64>,
    pub finished: Option<i64>,
    pub url: String,
}

/// One step of a job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimedStep {
    pub name: String,
    /// Its number in the job (GitHub), which its log is found by.
    pub number: u64,
    pub state: CheckState,
    pub started: Option<i64>,
    pub finished: Option<i64>,
}

/// One job of a run, with the times the forge recorded of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimedJob {
    pub id: u64,
    pub name: String,
    /// Its stage (GitLab; empty on GitHub).
    pub stage: String,
    pub state: CheckState,
    /// When the forge created it (GitHub queues a job once what it needs has finished).
    pub created: Option<i64>,
    /// Seconds it waited for a runner, when the forge says so (GitLab's `queued_duration`).
    pub queued: Option<i64>,
    pub started: Option<i64>,
    pub finished: Option<i64>,
    /// Its page and log on the forge.
    pub url: String,
    /// What the forge recorded of where it ran (runner, labels, group, tags), by name.
    pub runner: Vec<(String, String)>,
    pub steps: Vec<TimedStep>,
}

impl TimedJob {
    /// When it was waiting to start from: `queued` seconds before it started when the forge
    /// says how long it queued (GitLab, whose jobs are all created with their pipeline), else
    /// when it was created.
    pub fn ready(&self) -> Option<i64> {
        self.started
            .zip(self.queued)
            .map(|(s, q)| s - q)
            .or(self.created)
    }

    /// The page of step `step`'s log (GitHub), or the job's.
    pub fn step_url(&self, step: &TimedStep) -> String {
        if step.number > 0 && self.url.contains("/actions/runs/") {
            format!("{}#step:{}:1", self.url, step.number)
        } else {
            self.url.clone()
        }
    }
}

/// A run and its jobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimedRun {
    pub info: RunInfo,
    pub jobs: Vec<TimedJob>,
    /// The forge has more jobs than were read.
    pub more: bool,
}

/// Run `run` of `project` and its jobs.
pub fn timed_run(repo: &Path, project: &Project, run: u64) -> Result<TimedRun, String> {
    match project.forge {
        Forge::GitHub => {
            let info = parse_github_run(&gh_api(
                repo,
                project,
                &format!("repos/{}/actions/runs/{run}", project.path),
            )?)?;
            let jobs = gh_api(
                repo,
                project,
                &format!(
                    "repos/{}/actions/runs/{run}/jobs?per_page={MAX_JOBS}&filter=latest",
                    project.path
                ),
            )?;
            let (jobs, more) = parse_github_jobs(&jobs)?;
            Ok(TimedRun { info, jobs, more })
        }
        Forge::GitLab => {
            let info =
                parse_gitlab_pipeline(&glab_api(repo, project, &format!("pipelines/{run}"))?)?;
            let jobs = glab_api(
                repo,
                project,
                &format!("pipelines/{run}/jobs?per_page={MAX_JOBS}"),
            )?;
            let jobs = parse_gitlab_jobs(&jobs)?;
            let more = jobs.len() >= MAX_JOBS;
            Ok(TimedRun { info, jobs, more })
        }
        Forge::Forgejo | Forge::Gitea => Err(super::no_builds(project.forge)),
    }
}

/// The latest successful runs of `info`'s workflow on its branch, newest first, at most
/// `limit`: the runs it can be compared with.
pub fn successful_runs(
    repo: &Path,
    project: &Project,
    info: &RunInfo,
    limit: usize,
) -> Result<Vec<RunInfo>, String> {
    let branch = query_value(&info.branch);
    match project.forge {
        Forge::GitHub => {
            let Some(workflow) = info.workflow_id else {
                return Ok(Vec::new());
            };
            let mut rest = format!(
                "repos/{}/actions/workflows/{workflow}/runs?status=success&per_page={limit}",
                project.path
            );
            if !branch.is_empty() {
                rest.push_str(&format!("&branch={branch}"));
            }
            parse_github_runs(&gh_api(repo, project, &rest)?)
        }
        Forge::GitLab => {
            let mut rest = format!("pipelines?status=success&per_page={limit}");
            if !branch.is_empty() {
                rest.push_str(&format!("&ref={branch}"));
            }
            let json = glab_api(repo, project, &rest)?;
            Ok(json_list(&json)?.iter().map(gitlab_pipeline).collect())
        }
        Forge::Forgejo | Forge::Gitea => Err(super::no_builds(project.forge)),
    }
}

fn gh_api(repo: &Path, project: &Project, rest: &str) -> Result<String, String> {
    forge::tool(&project.hosting(), repo, &["api", rest])
}

fn glab_api(repo: &Path, project: &Project, rest: &str) -> Result<String, String> {
    let endpoint = format!("projects/{}/{rest}", project.path.replace('/', "%2F"));
    forge::tool(
        &project.hosting(),
        repo,
        &["api", "--hostname", &project.host, &endpoint],
    )
}

/// `text` as a value in a query: every byte but letters, digits and `-._~` escaped.
pub(crate) fn query_value(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("unexpected answer from the forge: {e}"))
}

fn json_list(text: &str) -> Result<Vec<Value>, String> {
    Ok(json(text)?.as_array().cloned().unwrap_or_default())
}

fn number(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn when(v: &Value, key: &str) -> Option<i64> {
    time(at(v, key))
}

/// The workflow file of a run's `path`, without the `@ref` a called workflow has; empty for
/// one that isn't a file of the repository (`dynamic/pages/…`).
fn workflow_path(path: &str) -> String {
    let path = path.split('@').next().unwrap_or_default();
    if path.starts_with(".github/workflows/") {
        path.to_owned()
    } else {
        String::new()
    }
}

fn github_run(run: &Value) -> RunInfo {
    let state = github::state(at(run, "/status"), at(run, "/conclusion"));
    RunInfo {
        id: number(run, "id"),
        name: at(run, "/name").to_owned(),
        number: number(run, "run_number"),
        attempt: number(run, "run_attempt").max(1),
        workflow_id: run.get("workflow_id").and_then(Value::as_u64),
        path: workflow_path(at(run, "/path")),
        branch: at(run, "/head_branch").to_owned(),
        sha: at(run, "/head_sha").to_owned(),
        event: at(run, "/event").to_owned(),
        state,
        created: when(run, "/created_at"),
        started: when(run, "/run_started_at").or_else(|| when(run, "/created_at")),
        finished: (state != CheckState::Running)
            .then(|| when(run, "/updated_at"))
            .flatten(),
        url: at(run, "/html_url").to_owned(),
    }
}

fn parse_github_run(text: &str) -> Result<RunInfo, String> {
    Ok(github_run(&json(text)?))
}

fn parse_github_runs(text: &str) -> Result<Vec<RunInfo>, String> {
    Ok(items(&json(text)?, "/workflow_runs")
        .iter()
        .map(github_run)
        .collect())
}

/// The jobs of a run, and whether the forge has more of them.
fn parse_github_jobs(text: &str) -> Result<(Vec<TimedJob>, bool), String> {
    let v = json(text)?;
    let jobs: Vec<TimedJob> = items(&v, "/jobs")
        .iter()
        .map(|job| {
            let mut runner = Vec::new();
            for (key, what) in [
                ("/runner_name", "runner"),
                ("/runner_group_name", "runner group"),
            ] {
                if !at(job, key).is_empty() {
                    runner.push((what.to_owned(), at(job, key).to_owned()));
                }
            }
            let labels: Vec<&str> = items(job, "/labels")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if !labels.is_empty() {
                runner.push(("labels".to_owned(), labels.join(", ")));
            }
            TimedJob {
                id: number(job, "id"),
                name: at(job, "/name").to_owned(),
                stage: String::new(),
                state: github::state(at(job, "/status"), at(job, "/conclusion")),
                created: when(job, "/created_at"),
                queued: None,
                started: when(job, "/started_at"),
                finished: when(job, "/completed_at"),
                url: at(job, "/html_url").to_owned(),
                runner,
                steps: items(job, "/steps")
                    .iter()
                    .map(|step| TimedStep {
                        name: at(step, "/name").to_owned(),
                        number: number(step, "number"),
                        state: github::state(at(step, "/status"), at(step, "/conclusion")),
                        started: when(step, "/started_at"),
                        finished: when(step, "/completed_at"),
                    })
                    .collect(),
            }
        })
        .collect();
    let total = v.get("total_count").and_then(Value::as_u64).unwrap_or(0);
    let more = total as usize > jobs.len();
    Ok((jobs, more))
}

fn gitlab_pipeline(pipeline: &Value) -> RunInfo {
    let state = gitlab::state(at(pipeline, "/status"));
    let name = match at(pipeline, "/name") {
        "" => "Pipeline",
        name => name,
    };
    RunInfo {
        id: number(pipeline, "id"),
        name: name.to_owned(),
        number: number(pipeline, "iid"),
        attempt: 1,
        workflow_id: None,
        path: ".gitlab-ci.yml".to_owned(),
        branch: at(pipeline, "/ref").to_owned(),
        sha: at(pipeline, "/sha").to_owned(),
        event: at(pipeline, "/source").to_owned(),
        state,
        created: when(pipeline, "/created_at"),
        started: when(pipeline, "/started_at").or_else(|| when(pipeline, "/created_at")),
        finished: (state != CheckState::Running)
            .then(|| when(pipeline, "/finished_at").or_else(|| when(pipeline, "/updated_at")))
            .flatten(),
        url: at(pipeline, "/web_url").to_owned(),
    }
}

fn parse_gitlab_pipeline(text: &str) -> Result<RunInfo, String> {
    Ok(gitlab_pipeline(&json(text)?))
}

/// The jobs of a pipeline, oldest first (their stages in the pipeline's order).
fn parse_gitlab_jobs(text: &str) -> Result<Vec<TimedJob>, String> {
    let mut jobs: Vec<TimedJob> = json_list(text)?
        .iter()
        .map(|job| {
            let mut runner = Vec::new();
            for (key, what) in [
                ("/runner/description", "runner"),
                ("/runner/runner_type", "runner type"),
            ] {
                if !at(job, key).is_empty() {
                    runner.push((what.to_owned(), at(job, key).to_owned()));
                }
            }
            let tags: Vec<&str> = items(job, "/tag_list")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            if !tags.is_empty() {
                runner.push(("tags".to_owned(), tags.join(", ")));
            }
            TimedJob {
                id: number(job, "id"),
                name: at(job, "/name").to_owned(),
                stage: at(job, "/stage").to_owned(),
                state: gitlab::state(at(job, "/status")),
                created: when(job, "/created_at"),
                queued: job
                    .get("queued_duration")
                    .and_then(Value::as_f64)
                    .map(|q| q.round() as i64),
                started: when(job, "/started_at"),
                finished: when(job, "/finished_at"),
                url: at(job, "/web_url").to_owned(),
                runner,
                steps: Vec::new(),
            }
        })
        .collect();
    jobs.sort_by_key(|job| job.id);
    Ok(jobs)
}

#[cfg(test)]
mod tests {
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
}
