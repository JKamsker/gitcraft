//! Deployments as the forges record them, through `gh api` and `glab api`: GitHub's
//! deployments with their statuses and its environments, GitLab's deployments and
//! environments; and the artifacts a run uploaded. Everything here only reads.

use std::path::Path;

use serde_json::Value;

use super::{Project, github, time};
use crate::forge::{self, Forge};
use crate::pull_request::{at, items};

/// So many of the latest deployments are read.
pub const MAX_DEPLOYMENTS: usize = 50;
/// So many deployments' statuses are read (GitHub asks for each one's apart).
pub const MAX_STATUS_READS: usize = 30;
/// So many deployments of an environment are looked through for its latest successful one.
pub const MAX_PER_ENVIRONMENT: usize = 5;

/// Where a deployment stands, by its latest status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeployState {
    Success,
    InProgress,
    /// Queued, pending, or waiting for an approval.
    Queued,
    Failed,
    /// It succeeded once and a later deployment replaced it.
    Inactive,
    Cancelled,
    /// The forge has no status for it.
    NoStatus,
    /// Its status wasn't read (a limit was reached).
    NotRead,
}

impl DeployState {
    pub fn label(self) -> &'static str {
        match self {
            DeployState::Success => "succeeded",
            DeployState::InProgress => "in progress",
            DeployState::Queued => "queued",
            DeployState::Failed => "failed",
            DeployState::Inactive => "replaced",
            DeployState::Cancelled => "cancelled",
            DeployState::NoStatus => "no status",
            DeployState::NotRead => "status not read",
        }
    }

    /// Still to come: queued or under way, as its status says. One with no status says
    /// nothing either way (it may be an old one that never got any).
    pub fn in_flight(self) -> bool {
        matches!(self, DeployState::InProgress | DeployState::Queued)
    }
}

/// A GitHub deployment status's `state`.
pub fn github_state(state: &str) -> DeployState {
    match state {
        "success" => DeployState::Success,
        "in_progress" => DeployState::InProgress,
        "queued" | "pending" | "waiting" => DeployState::Queued,
        "failure" | "error" => DeployState::Failed,
        "inactive" => DeployState::Inactive,
        _ => DeployState::NoStatus,
    }
}

/// A GitLab deployment's `status`.
pub fn gitlab_state(status: &str) -> DeployState {
    match status {
        "success" => DeployState::Success,
        "running" => DeployState::InProgress,
        "created" | "blocked" | "waiting" => DeployState::Queued,
        "failed" => DeployState::Failed,
        "canceled" | "skipped" => DeployState::Cancelled,
        _ => DeployState::NoStatus,
    }
}

/// One deployment of a commit to an environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deployment {
    pub id: u64,
    pub environment: String,
    /// The commit deployed, as the forge recorded it.
    pub sha: String,
    pub reference: String,
    pub created: Option<i64>,
    pub state: Option<DeployState>,
    /// When its latest status was reported.
    pub reported: Option<i64>,
    /// Its page on the forge: the log, the job, or the environment.
    pub url: String,
    /// Where it was deployed to, when the forge says.
    pub environment_url: String,
    /// The run (GitHub) or pipeline (GitLab) that did it, when the record names one.
    pub run: Option<u64>,
}

impl Deployment {
    pub fn state(&self) -> DeployState {
        self.state.unwrap_or(DeployState::NotRead)
    }
}

/// An environment the forge lists, deployed to or not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    pub name: String,
    /// Its page on the forge.
    pub url: String,
}

/// What the forge recorded of deployments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deployments {
    pub environments: Vec<Environment>,
    /// Newest first.
    pub deployments: Vec<Deployment>,
    /// Why it is incomplete, when it is.
    pub partial: Vec<String>,
}

/// One artifact a run uploaded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Artifact {
    pub id: u64,
    pub name: String,
    pub run: u64,
    /// Its digest (`sha256:…`), when the forge recorded one.
    pub digest: Option<String>,
    pub size: Option<u64>,
    pub expired: bool,
    pub url: String,
}

/// The deployments and environments of `project`.
pub fn read(repo: &Path, project: &Project) -> Result<Deployments, String> {
    let hosting = project.hosting();
    match project.forge {
        Forge::GitHub => read_github(project, |rest| forge::tool(&hosting, repo, &["api", rest])),
        Forge::GitLab => read_gitlab(project, |rest| {
            let endpoint = format!("projects/{}/{rest}", project.path.replace('/', "%2F"));
            forge::tool(
                &hosting,
                repo,
                &["api", "--hostname", &project.host, &endpoint],
            )
        }),
        Forge::Forgejo | Forge::Gitea => Err(no_deployments(project.forge)),
    }
}

/// What is said for a forge whose deployments gitcraft doesn't read.
pub fn no_deployments(forge: Forge) -> String {
    format!(
        "gitcraft doesn't read the deployments of {} yet",
        forge.name()
    )
}

fn json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("unexpected answer from the forge: {e}"))
}

fn number(v: &Value, pointer: &str) -> Option<u64> {
    v.pointer(pointer).and_then(Value::as_u64)
}

fn web(project: &Project) -> String {
    format!("https://{}/{}", project.host, project.path)
}

/// GitHub: the environments, the latest deployments, and the statuses of each environment's
/// latest deployments back to its latest success. `api` answers `gh api <rest>`.
pub(crate) fn read_github(
    project: &Project,
    mut api: impl FnMut(&str) -> Result<String, String>,
) -> Result<Deployments, String> {
    let list = api(&format!(
        "repos/{}/deployments?per_page={MAX_DEPLOYMENTS}",
        project.path
    ))?;
    let mut out = Deployments {
        deployments: parse_github_deployments(&list, project)?,
        ..Deployments::default()
    };
    match api(&format!("repos/{}/environments?per_page=100", project.path))
        .and_then(|text| parse_github_environments(&text, project))
    {
        Ok(envs) => out.environments = envs,
        Err(e) => out
            .partial
            .push(format!("the environments couldn't be listed: {e}")),
    }
    if out.deployments.len() >= MAX_DEPLOYMENTS {
        out.partial.push(format!(
            "only the latest {MAX_DEPLOYMENTS} deployments were read"
        ));
    }
    let mut reads = 0;
    let mut cut = false;
    let names = environment_order(&out.deployments);
    for name in names {
        let mine: Vec<usize> = (0..out.deployments.len())
            .filter(|&i| out.deployments[i].environment == name)
            .take(MAX_PER_ENVIRONMENT)
            .collect();
        for i in mine {
            if reads == MAX_STATUS_READS {
                cut = true;
                break;
            }
            reads += 1;
            let id = out.deployments[i].id;
            let text = api(&format!(
                "repos/{}/deployments/{id}/statuses?per_page=1",
                project.path
            ))?;
            apply_github_status(&mut out.deployments[i], &text)?;
            if out.deployments[i].state == Some(DeployState::Success) {
                break;
            }
        }
    }
    if cut {
        out.partial.push(format!(
            "the statuses of only {MAX_STATUS_READS} deployments were read"
        ));
    }
    Ok(out)
}

/// The environments in the order their newest deployment came.
fn environment_order(deployments: &[Deployment]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for d in deployments {
        if !names.contains(&d.environment) {
            names.push(d.environment.clone());
        }
    }
    names
}

fn parse_github_deployments(text: &str, project: &Project) -> Result<Vec<Deployment>, String> {
    let v = json(text)?;
    let list = v
        .as_array()
        .ok_or("unexpected answer from the forge: not a list")?;
    Ok(list
        .iter()
        .map(|d| {
            let environment = at(d, "/environment").to_owned();
            Deployment {
                id: number(d, "/id").unwrap_or(0),
                url: format!(
                    "{}/deployments/{}",
                    web(project),
                    super::timing::query_value(&environment)
                ),
                environment,
                sha: at(d, "/sha").to_owned(),
                reference: at(d, "/ref").to_owned(),
                created: time(at(d, "/created_at")),
                state: None,
                reported: None,
                environment_url: String::new(),
                run: None,
            }
        })
        .collect())
}

/// Takes the latest of a deployment's statuses (GitHub lists the newest first).
fn apply_github_status(deployment: &mut Deployment, text: &str) -> Result<(), String> {
    let v = json(text)?;
    let Some(status) = v.as_array().and_then(|l| l.first()) else {
        deployment.state = Some(DeployState::NoStatus);
        return Ok(());
    };
    deployment.state = Some(github_state(at(status, "/state")));
    deployment.reported =
        time(at(status, "/updated_at")).or_else(|| time(at(status, "/created_at")));
    let log = [at(status, "/log_url"), at(status, "/target_url")]
        .into_iter()
        .find(|u| u.starts_with("https://"));
    if let Some(log) = log {
        deployment.url = log.to_owned();
        deployment.run = github::run_of_url(log);
    }
    deployment.environment_url = at(status, "/environment_url").to_owned();
    Ok(())
}

fn parse_github_environments(text: &str, project: &Project) -> Result<Vec<Environment>, String> {
    let v = json(text)?;
    Ok(items(&v, "/environments")
        .iter()
        .map(|e| {
            let name = at(e, "/name").to_owned();
            let url = match at(e, "/html_url") {
                u if u.starts_with("https://") => u.to_owned(),
                _ => format!(
                    "{}/deployments/{}",
                    web(project),
                    super::timing::query_value(&name)
                ),
            };
            Environment { name, url }
        })
        .collect())
}

/// GitLab: the environments and the latest deployments, each with its status. `api` answers
/// `glab api projects/<project>/<rest>`.
pub(crate) fn read_gitlab(
    project: &Project,
    mut api: impl FnMut(&str) -> Result<String, String>,
) -> Result<Deployments, String> {
    let list = api(&format!(
        "deployments?order_by=id&sort=desc&per_page={MAX_DEPLOYMENTS}"
    ))?;
    let mut out = Deployments {
        deployments: parse_gitlab_deployments(&list, project)?,
        ..Deployments::default()
    };
    match api("environments?per_page=100").and_then(|t| parse_gitlab_environments(&t, project)) {
        Ok(envs) => out.environments = envs,
        Err(e) => out
            .partial
            .push(format!("the environments couldn't be listed: {e}")),
    }
    if out.deployments.len() >= MAX_DEPLOYMENTS {
        out.partial.push(format!(
            "only the latest {MAX_DEPLOYMENTS} deployments were read"
        ));
    }
    Ok(out)
}

fn parse_gitlab_deployments(text: &str, project: &Project) -> Result<Vec<Deployment>, String> {
    let v = json(text)?;
    let list = v
        .as_array()
        .ok_or("unexpected answer from the forge: not a list")?;
    Ok(list
        .iter()
        .map(|d| {
            let job = at(d, "/deployable/web_url");
            let env_id = number(d, "/environment/id");
            let url = if job.starts_with("https://") || job.starts_with("http://") {
                job.to_owned()
            } else {
                match env_id {
                    Some(id) => format!("{}/-/environments/{id}", web(project)),
                    None => format!("{}/-/environments", web(project)),
                }
            };
            Deployment {
                id: number(d, "/id").unwrap_or(0),
                environment: at(d, "/environment/name").to_owned(),
                sha: at(d, "/sha").to_owned(),
                reference: at(d, "/ref").to_owned(),
                created: time(at(d, "/created_at")),
                state: Some(gitlab_state(at(d, "/status"))),
                reported: time(at(d, "/updated_at")).or_else(|| time(at(d, "/created_at"))),
                url,
                environment_url: at(d, "/environment/external_url").to_owned(),
                run: number(d, "/deployable/pipeline/id"),
            }
        })
        .collect())
}

fn parse_gitlab_environments(text: &str, project: &Project) -> Result<Vec<Environment>, String> {
    let v = json(text)?;
    let list = v
        .as_array()
        .ok_or("unexpected answer from the forge: not a list")?;
    Ok(list
        .iter()
        .map(|e| Environment {
            name: at(e, "/name").to_owned(),
            url: match number(e, "/id") {
                Some(id) => format!("{}/-/environments/{id}", web(project)),
                None => format!("{}/-/environments", web(project)),
            },
        })
        .collect())
}

/// The artifacts run `run` uploaded: GitHub's with their digests, GitLab's from its jobs
/// (without digests: GitLab records none).
pub fn artifacts(repo: &Path, project: &Project, run: u64) -> Result<Vec<Artifact>, String> {
    let hosting = project.hosting();
    match project.forge {
        Forge::GitHub => {
            let rest = format!(
                "repos/{}/actions/runs/{run}/artifacts?per_page=50",
                project.path
            );
            parse_github_artifacts(&forge::tool(&hosting, repo, &["api", &rest])?, project, run)
        }
        Forge::GitLab => {
            let endpoint = format!(
                "projects/{}/pipelines/{run}/jobs?per_page=100",
                project.path.replace('/', "%2F")
            );
            let text = forge::tool(
                &hosting,
                repo,
                &["api", "--hostname", &project.host, &endpoint],
            )?;
            parse_gitlab_artifacts(&text, run)
        }
        Forge::Forgejo | Forge::Gitea => Err(no_deployments(project.forge)),
    }
}

fn parse_github_artifacts(
    text: &str,
    project: &Project,
    run: u64,
) -> Result<Vec<Artifact>, String> {
    let v = json(text)?;
    Ok(items(&v, "/artifacts")
        .iter()
        .map(|a| {
            let id = number(a, "/id").unwrap_or(0);
            Artifact {
                id,
                name: at(a, "/name").to_owned(),
                run,
                digest: Some(at(a, "/digest"))
                    .filter(|d| !d.is_empty())
                    .map(str::to_owned),
                size: number(a, "/size_in_bytes"),
                expired: a.get("expired").and_then(Value::as_bool).unwrap_or(false),
                url: format!("{}/actions/runs/{run}/artifacts/{id}", web(project)),
            }
        })
        .collect())
}

fn parse_gitlab_artifacts(text: &str, run: u64) -> Result<Vec<Artifact>, String> {
    let v = json(text)?;
    let list = v
        .as_array()
        .ok_or("unexpected answer from the forge: not a list")?;
    Ok(list
        .iter()
        .filter(|job| job.pointer("/artifacts_file/filename").is_some())
        .map(|job| Artifact {
            id: number(job, "/id").unwrap_or(0),
            name: format!(
                "{}: {}",
                at(job, "/name"),
                at(job, "/artifacts_file/filename")
            ),
            run,
            digest: None,
            size: number(job, "/artifacts_file/size"),
            expired: false,
            url: format!("{}/artifacts/browse", at(job, "/web_url")),
        })
        .collect())
}

#[cfg(test)]
mod tests;
