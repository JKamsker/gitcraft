//! GitLab merge requests, native discussions, approvals and SHA-guarded merges through glab.
mod api;
mod details;
mod post;
#[cfg(test)]
mod native_tests;

use std::path::Path;

use serde_json::Value;

use super::{Hosting, NewPullRequest, Pages, Provider, PullRequest, cli};
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

    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
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
pub(super) fn api_authority(hosting: &Hosting) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

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

impl Pages for GitLab {
    fn details(
        &self,
        repo: &Path,
        hosting: &Hosting,
        number: u64,
    ) -> Result<crate::pull_request::PrDetails, String> {
        details::read(&api::Api::new(repo, hosting)?, number)
    }
    fn target(
        &self,
        repo: &Path,
        hosting: &Hosting,
        details: &crate::pull_request::PrDetails,
    ) -> Option<Box<dyn crate::pr_post::Target>> {
        Some(Box::new(post::GitLabTarget::new(
            api::Api::new(repo, hosting).ok()?,
            details,
        )?))
    }
}
