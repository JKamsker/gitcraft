//! The timings of one run in more detail than [`super::jobs`] reads them: when each job was
//! queued as well as when it started and ended, what the forge recorded of the runner it ran
//! on, the workflow file the run came from, and the earlier successful runs of the same
//! workflow, to compare it with. Everything here only reads.

use std::path::Path;

use serde_json::Value;

use super::{CheckState, Project, github, gitlab, time};
use crate::forge::{self, Forge};
use crate::pull_request::{at, items};

pub mod history;
pub mod logs;

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
            history::attempt(repo, project, history::RunKey::of(&info))
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
        Forge::Forgejo | Forge::Gitea | Forge::Azure | Forge::Bitbucket => {
            Err(super::no_builds(project.forge))
        }
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
        Forge::Forgejo | Forge::Gitea | Forge::Azure | Forge::Bitbucket => {
            Err(super::no_builds(project.forge))
        }
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
mod tests;
