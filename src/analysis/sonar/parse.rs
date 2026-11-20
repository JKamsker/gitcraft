//! Pure parsers for the Web API answers used. Shapes follow each endpoint's documented
//! response example (`api/webservices/response_example`) on SonarQube Server 24.12/26.x and
//! SonarQube Cloud; fields added or removed across versions are optional here.

use serde_json::Value;

use super::super::Failure;
use super::super::gate::{Analysis, ServerGate};

/// `major.minor` of `api/server/version` (plain text, e.g. `10.4.1.88267` or `2025.1.0.1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Version {
    pub major: u32,
    pub minor: u32,
}

pub(super) fn version(text: &str) -> Option<Version> {
    let mut parts = text.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |m| m.parse().ok())?;
    Some(Version { major, minor })
}

fn json(body: &str) -> Result<Value, Failure> {
    serde_json::from_str(body).map_err(|_| Failure::Malformed)
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, Failure> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or(Failure::Malformed)
}

/// `api/authentication/validate`: `{"valid": true}`.
pub(super) fn valid(body: &str) -> Result<bool, Failure> {
    json(body)?
        .get("valid")
        .and_then(Value::as_bool)
        .ok_or(Failure::Malformed)
}

/// `api/components/show` on Cloud: the project's `component.organization`.
pub(super) fn organization(body: &str) -> Result<Option<String>, Failure> {
    let v = json(body)?;
    let component = v.get("component").ok_or(Failure::Malformed)?;
    Ok(text(component, "organization").map(str::to_owned))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Branch {
    pub name: String,
    pub is_main: bool,
}

/// `api/project_branches/list` (since 6.6): every branch of the project, not paginated.
pub(super) fn branches(body: &str) -> Result<Vec<Branch>, Failure> {
    let v = json(body)?;
    rows(&v, "branches")?
        .iter()
        .map(|row| {
            Ok(Branch {
                name: text(row, "name").ok_or(Failure::Malformed)?.to_owned(),
                is_main: row.get("isMain").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PullRequest {
    pub key: String,
    pub date: Option<String>,
    /// `commit.sha`; absent before the PR's first analysis and on some versions.
    pub sha: Option<String>,
}

/// `api/project_pull_requests/list`: every analyzed pull request, not paginated.
pub(super) fn pull_requests(body: &str) -> Result<Vec<PullRequest>, Failure> {
    let v = json(body)?;
    rows(&v, "pullRequests")?
        .iter()
        .map(|row| {
            Ok(PullRequest {
                key: text(row, "key").ok_or(Failure::Malformed)?.to_owned(),
                date: text(row, "analysisDate").map(str::to_owned),
                sha: row
                    .get("commit")
                    .and_then(|c| text(c, "sha"))
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// `api/project_analyses/search` asked with `ps=1`: the newest analysis (the endpoint sorts
/// by date, newest first), with `revision` when the scanner recorded one.
pub(super) fn latest_analysis(body: &str) -> Result<Option<Analysis>, Failure> {
    let v = json(body)?;
    let Some(row) = rows(&v, "analyses")?.first() else {
        return Ok(None);
    };
    Ok(Some(Analysis {
        key: Some(text(row, "key").ok_or(Failure::Malformed)?.to_owned()),
        date: text(row, "date").map(str::to_owned),
        revision: text(row, "revision").map(str::to_owned),
    }))
}

/// `api/qualitygates/project_status`: `OK`, `ERROR`, `NONE`, and `WARN` before 7.6, plus the
/// metric keys of the failing conditions.
pub(super) fn project_status(body: &str) -> Result<(ServerGate, Vec<String>), Failure> {
    let v = json(body)?;
    let status = v.get("projectStatus").ok_or(Failure::Malformed)?;
    let gate = match text(status, "status") {
        Some("OK") => ServerGate::Ok,
        Some("ERROR") => ServerGate::Error,
        Some("WARN") => ServerGate::Warn,
        Some("NONE") => ServerGate::None,
        _ => ServerGate::Unknown,
    };
    let failing = status
        .get("conditions")
        .and_then(Value::as_array)
        .map(|conditions| {
            conditions
                .iter()
                .filter(|c| matches!(text(c, "status"), Some("ERROR" | "WARN")))
                .filter_map(|c| text(c, "metricKey").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok((gate, failing))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Task {
    pub status: String,
    /// `branch` / `pullRequest` (since 6.6); both absent for the main branch on editions
    /// without branch analysis.
    pub branch: Option<String>,
    pub pull_request: Option<String>,
    /// From `current` (the last executed task) rather than `queue`.
    pub current: bool,
}

/// `api/ce/component`: queued and in-progress tasks plus the last executed one.
pub(super) fn tasks(body: &str) -> Result<Vec<Task>, Failure> {
    let v = json(body)?;
    let task = |row: &Value, current| Task {
        status: text(row, "status").unwrap_or_default().to_owned(),
        branch: text(row, "branch").map(str::to_owned),
        pull_request: text(row, "pullRequest").map(str::to_owned),
        current,
    };
    let mut all: Vec<Task> = v
        .get("queue")
        .and_then(Value::as_array)
        .map(|q| q.iter().map(|row| task(row, false)).collect())
        .unwrap_or_default();
    if let Some(current) = v.get("current").filter(|c| c.is_object()) {
        all.push(task(current, true));
    }
    Ok(all)
}
