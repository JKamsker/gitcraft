//! GitLab project identifiers, reviewers, issues, and merge-request response parsing.
use super::*;

pub(super) fn gitlab_reviewers(reviewers: &[String]) -> Vec<String> {
    reviewers
        .iter()
        .filter(|r| !r.contains('/'))
        .cloned()
        .collect()
}

/// Where a self-hosted GitLab's API is: its host, with the port of an `https://host:8443/…`
/// remote (its web pages and API are on that port too).
pub(in crate::forge) fn api_authority(hosting: &Hosting) -> String {
    hosting
        .web
        .split_once("://")
        .map_or(hosting.host.as_str(), |(_, authority)| authority)
        .to_owned()
}

/// `group%2Fsub%2Fproject`, how the API names a project by its path; none for a path that
/// isn't plain (it goes into the API's path as it is).
pub(super) fn project_id(path: &str) -> Option<String> {
    let plain = path.split('/').all(|part| {
        !part.is_empty()
            && !part.starts_with(['-', '.'])
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    });
    (plain && path.contains('/')).then(|| path.replace('/', "%2F"))
}

pub(super) fn parse_list(json: &str) -> Result<Vec<PullRequest>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| {
        crate::i18n::message(
            "backend-unexpected-tool-output-e-164ff7",
            &[
                ("tool", (TOOL).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    let text = |v: &Value, pointer: &str| {
        v.pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| PullRequest {
            number: item.get("iid").and_then(Value::as_u64).unwrap_or(0),
            title: text(item, "/title"),
            branch: text(item, "/source_branch"),
            base: text(item, "/target_branch"),
            base_oid: String::new(),
            from_fork: item.get("source_project_id") != item.get("target_project_id"),
            sha: text(item, "/sha"),
            author: text(item, "/author/username"),
            url: text(item, "/web_url"),
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect())
}

/// glab's list of issues.
pub(super) fn parse_issues(json: &str) -> Result<Vec<Issue>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| {
        crate::i18n::message(
            "backend-unexpected-tool-output-e-164ff7",
            &[
                ("tool", (TOOL).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    let list = value.as_array().ok_or_else(|| {
        crate::i18n::message(
            "backend-unexpected-tool-output-not-a-list-of-issues-92f726",
            &[("tool", (TOOL).to_string().into())],
        )
    })?;
    Ok(list.iter().filter_map(parse_issue).collect())
}

/// One issue: `iid` is the number `#N` means in the project (`id` is the server's).
pub(super) fn parse_issue(item: &Value) -> Option<Issue> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Some(Issue {
        tracker: None,
        acceptance: None,
        number: item.get("iid").and_then(Value::as_u64)?,
        title: text("title"),
        body: text("description"),
        url: text("web_url"),
        labels: item
            .get("labels")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    })
}
