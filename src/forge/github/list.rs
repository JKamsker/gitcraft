//! Complete, repository-scoped open PR enumeration, independent of the viewer and local refs.
use std::{collections::HashSet, path::Path};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Hosting, PullRequest, gh_with_input};

const QUERY: &str = "query($owner: String!, $name: String!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 100, after: $after) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title headRefName baseRefName baseRefOid headRefOid
        author { login } url isDraft isCrossRepository
        headRepository { nameWithOwner }
      }
    }
  }
}";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Node {
    number: u64,
    title: String,
    head_ref_name: String,
    base_ref_name: String,
    base_ref_oid: String,
    head_ref_oid: String,
    author: Option<Author>,
    url: String,
    is_draft: bool,
    is_cross_repository: bool,
    head_repository: Option<Repository>,
}
#[derive(Deserialize)]
struct Author {
    login: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    nodes: Vec<Node>,
    page_info: PageInfo,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

pub(super) fn fetch(repo: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
    let (owner, name) = hosting
        .path
        .split_once('/')
        .ok_or("Invalid GitHub repository")?;
    let mut after: Option<String> = None;
    let mut cursors = HashSet::new();
    let mut numbers = HashSet::new();
    let mut prs = Vec::new();
    loop {
        let payload = json!({"query": QUERY, "variables": {
            "owner": owner, "name": name, "after": after
        }});
        let out = gh_with_input(
            repo,
            hosting,
            &["api", "graphql", "--input", "-"],
            Some(&payload.to_string()),
        )?;
        let page = parse(&out)?;
        for node in page.nodes {
            if node.number == 0 {
                return Err("Invalid GitHub pull request number".into());
            }
            if numbers.insert(node.number) {
                prs.push(node.into());
            }
        }
        if !page.page_info.has_next_page {
            return Ok(prs);
        }
        let cursor = page
            .page_info
            .end_cursor
            .filter(|s| !s.trim().is_empty())
            .ok_or("Missing GitHub pagination cursor")?;
        if !cursors.insert(cursor.clone()) {
            return Err("Repeated GitHub pagination cursor".into());
        }
        after = Some(cursor);
    }
}

fn parse(out: &str) -> Result<Page, String> {
    let value: Value = serde_json::from_str(out).map_err(|e| e.to_string())?;
    if let Some(errors) = value.get("errors")
        && errors.as_array().is_none_or(|a| !a.is_empty())
    {
        return Err(format!("GitHub GraphQL errors: {errors}"));
    }
    let page = value
        .pointer("/data/repository/pullRequests")
        .ok_or("Missing GitHub pull request connection")?;
    serde_json::from_value(page.clone()).map_err(|e| format!("Invalid GitHub PR list: {e}"))
}

impl From<Node> for PullRequest {
    fn from(n: Node) -> Self {
        Self {
            number: n.number,
            title: n.title,
            branch: n.head_ref_name,
            base: n.base_ref_name,
            base_oid: n.base_ref_oid,
            sha: n.head_ref_oid,
            from_fork: n.is_cross_repository,
            head_repo: n.head_repository.map(|r| r.name_with_owner),
            author: n.author.map_or_else(
                || "ghost".into(),
                |a| a.login.strip_prefix("app/").unwrap_or(&a.login).to_owned(),
            ),
            url: n.url,
            draft: n.is_draft,
        }
    }
}

#[cfg(test)]
mod tests;
