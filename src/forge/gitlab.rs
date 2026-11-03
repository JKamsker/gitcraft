//! GitLab, through `glab`: the list of merge requests and opening one. Their page is the web's.

use std::path::Path;

use serde_json::Value;

use super::{Hosting, NewPullRequest, Provider, PullRequest, cli};
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
        parse_list(&self.run(repo, &["mr", "list", "--output", "json"])?)
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

    /// The host's own avatar API, asked by email.
    fn avatars(&self, _: &Path, hosting: &Hosting, emails: &[String]) -> Vec<(String, Hint)> {
        emails
            .iter()
            .map(|email| (email.clone(), Hint::GitLab(hosting.host.clone())))
            .collect()
    }
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
        let gl = r#"[{"iid":3,"title":"Add","source_branch":"feat","author":{"username":"bo"},"web_url":"u","draft":false}]"#;
        let mrs = parse_list(gl).unwrap_or_default();
        assert_eq!((mrs[0].number, mrs[0].author.as_str()), (3, "bo"));
        assert_eq!(mrs[0].branch, "feat");
        assert!(parse_list("oops").is_err());
    }
}
