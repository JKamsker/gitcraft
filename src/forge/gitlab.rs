//! GitLab, through `glab`: the list of merge requests and opening one (their page is the web's),
//! and the issues assigned to the user.

use std::path::Path;

use serde_json::Value;

use super::{Hosting, Issue, MergedPullRequest, NewPullRequest, Provider, PullRequest, cli};
use crate::avatars::Hint;

pub(super) struct GitLab;

const TOOL: &str = "glab";

impl GitLab {
    fn run(&self, repo: &Path, args: &[&str]) -> Result<String, String> {
        cli::run(TOOL, self.noun(), repo, args)
    }
}

impl Provider for GitLab {
    fn name(&self) -> &'static str {
        "GitLab"
    }

    fn noun(&self) -> &'static str {
        "merge request"
    }

    fn claims(&self, host: &str) -> bool {
        host.contains("gitlab")
    }

    fn list(&self, repo: &Path, _: &Hosting) -> Result<Vec<PullRequest>, String> {
        // glab gives 30 at a time otherwise; 100, as GitHub's and Forgejo's lists have.
        let args = ["mr", "list", "--output", "json", "--per-page", "100"];
        parse_list(&self.run(repo, &args)?)
    }

    fn merged(&self, repo: &Path, _: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        let args = [
            "mr",
            "list",
            "--merged",
            "--output",
            "json",
            "--per-page",
            "100",
        ];
        let pointers = ["/iid", "/source_branch", "/sha", "/target_branch"];
        super::parse_merged(&self.run(repo, &args)?, pointers, TOOL)
    }

    fn create(&self, repo: &Path, _: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        let mut args = vec![
            "mr",
            "create",
            "--source-branch",
            new.branch,
            "--target-branch",
            new.base,
            "--title",
            new.title,
            "--description",
            new.body,
            "--yes",
        ];
        if new.draft {
            args.push("--draft");
        }
        self.run(repo, &args).map(|out| out.trim().to_owned())
    }

    /// `@me` is the account `glab` is signed in to for the host.
    fn assigned_issues(&self, repo: &Path, _: &Hosting) -> Result<Vec<Issue>, String> {
        let max = super::ISSUES_MAX.to_string();
        let args = [
            "issue",
            "list",
            "--assignee=@me",
            "--output",
            "json",
            "--per-page",
            &max,
        ];
        parse_issues(&cli::run(TOOL, "issue", repo, &args)?)
    }

    fn issue(&self, repo: &Path, _: &Hosting, number: u64) -> Result<Issue, String> {
        let number = number.to_string();
        let args = ["issue", "view", &number, "--output", "json"];
        let out = cli::run(TOOL, "issue", repo, &args)?;
        let value: Value =
            serde_json::from_str(&out).map_err(|e| format!("unexpected {TOOL} output: {e}"))?;
        parse_issue(&value).ok_or_else(|| format!("unexpected {TOOL} output: not an issue"))
    }

    /// The host's own avatar API, asked by email.
    fn avatars(&self, _: &Path, hosting: &Hosting, emails: &[String]) -> Vec<(String, Hint)> {
        let api = api_authority(hosting);
        emails
            .iter()
            .map(|email| (email.clone(), Hint::GitLab(api.clone())))
            .collect()
    }
}

/// Where a self-hosted GitLab's API is: its host, with the port of an `https://host:8443/…`
/// remote (its web pages and API are on that port too).
fn api_authority(hosting: &Hosting) -> String {
    hosting
        .web
        .split_once("://")
        .map_or(hosting.host.as_str(), |(_, authority)| authority)
        .to_owned()
}

fn parse_list(json: &str) -> Result<Vec<PullRequest>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected {TOOL} output: {e}"))?;
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
            sha: text(item, "/sha"),
            author: text(item, "/author/username"),
            url: text(item, "/web_url"),
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect())
}

/// glab's list of issues.
fn parse_issues(json: &str) -> Result<Vec<Issue>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected {TOOL} output: {e}"))?;
    let list = value
        .as_array()
        .ok_or_else(|| format!("unexpected {TOOL} output: not a list of issues"))?;
    Ok(list.iter().filter_map(parse_issue).collect())
}

/// One issue: `iid` is the number `#N` means in the project (`id` is the server's).
fn parse_issue(item: &Value) -> Option<Issue> {
    let text = |key: &str| {
        item.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    Some(Issue {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issues() {
        let gl = r###"[
            {"id": 9001, "iid": 12, "title": "Crash on login",
             "description": "## Acceptance criteria\n- [ ] works",
             "web_url": "https://gitlab.com/g/p/-/issues/12", "labels": ["bug", "ui"]},
            {"id": 9002, "iid": 13, "title": "Bare", "description": null, "web_url": "u",
             "labels": []},
            {"id": 9003, "title": "no iid"}
        ]"###;
        let issues = parse_issues(gl).unwrap_or_default();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 12);
        assert_eq!(issues[0].body, "## Acceptance criteria\n- [ ] works");
        assert_eq!(issues[0].url, "https://gitlab.com/g/p/-/issues/12");
        assert_eq!(issues[0].labels, ["bug", "ui"]);
        assert_eq!((issues[1].number, issues[1].body.as_str()), (13, ""));
        assert!(parse_issues("oops").is_err());
        assert!(parse_issues(r#"{"message": "401 Unauthorized"}"#).is_err());
    }

    #[test]
    fn parses_merge_requests() {
        let gl = r#"[{"iid":3,"title":"Add","source_branch":"feat","sha":"beef","author":{"username":"bo"},"web_url":"u","draft":false}]"#;
        let mrs = parse_list(gl).unwrap_or_default();
        assert_eq!((mrs[0].number, mrs[0].author.as_str()), (3, "bo"));
        assert_eq!(
            (mrs[0].branch.as_str(), mrs[0].sha.as_str()),
            ("feat", "beef")
        );
        assert!(parse_list("oops").is_err());
    }

    #[test]
    fn avatars_are_asked_on_the_remotes_port() {
        let at = |url: &str| {
            let origin = crate::git::Remote {
                name: "origin".into(),
                url: url.into(),
            };
            super::super::detect_with(&[origin], &Default::default(), &[])
                .map(|hosting| api_authority(&hosting))
                .unwrap_or_default()
        };
        assert_eq!(
            at("https://gitlab.example.com:8443/g/p.git"),
            "gitlab.example.com:8443"
        );
        assert_eq!(at("git@gitlab.com:g/p.git"), "gitlab.com");
        // An SSH port says nothing about the web pages'.
        assert_eq!(
            at("ssh://git@gitlab.example.com:2222/g/p.git"),
            "gitlab.example.com"
        );
    }
}
