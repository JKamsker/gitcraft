//! GitHub, through `gh`: the list of pull requests, opening one, and the page of one in full —
//! reading it ([`details`], [`threads`]) and acting on it ([`post`]).

mod details;
mod post;
mod threads;

use std::path::Path;

use serde_json::Value;

use super::inbox;
use super::{Hosting, NewPullRequest, Pages, Provider, PullRequest, cli};
use crate::avatars::Hint;
use crate::pr_post::Target;
use crate::pull_request::{PrDetails, at};

pub(super) struct GitHub;

const TOOL: &str = "gh";
const NOUN: &str = "pull request";

fn gh(repo: &Path, args: &[&str]) -> Result<String, String> {
    cli::run(TOOL, NOUN, repo, args)
}

/// [`gh`], with `input` on its stdin.
fn gh_with_input(repo: &Path, args: &[&str], input: Option<&str>) -> Result<String, String> {
    cli::run_with_input(TOOL, NOUN, repo, args, input)
}

/// The account at `pointer`: a deleted one has none, and GitHub calls those "ghost".
fn login(v: &Value, pointer: &str) -> String {
    match at(v, pointer) {
        "" => "ghost".to_owned(),
        name => name.strip_prefix("app/").unwrap_or(name).to_owned(),
    }
}

impl Provider for GitHub {
    fn name(&self) -> &'static str {
        "GitHub"
    }

    /// By the host alone: a GitLab project may well be called `me.github.io`, and a host naming
    /// GitLab is GitLab's (`github-mirror.gitlab.example.org`).
    fn claims(&self, host: &str) -> bool {
        host.contains("github") && !host.contains("gitlab")
    }

    fn list(&self, repo: &Path, _: &Hosting) -> Result<Vec<PullRequest>, String> {
        let out = gh(
            repo,
            &[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,headRefName,headRefOid,author,url,isDraft",
            ],
        )?;
        parse_list(&out)
    }

    fn create(&self, repo: &Path, _: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        let mut args = vec![
            "pr", "create", "--head", new.branch, "--base", new.base, "--title", new.title,
            "--body", new.body,
        ];
        if new.draft {
            args.push("--draft");
        }
        gh(repo, &args).map(|out| out.trim().to_owned())
    }

    /// The authors' GitHub accounts, from the repository's latest commits (the API ties each
    /// commit to the account its email belongs to). Through `gh` when it's installed (private
    /// repositories, higher limits), else GitHub's public API.
    fn avatars(&self, repo: &Path, hosting: &Hosting, _: &[String]) -> Vec<(String, Hint)> {
        let (host, path) = (hosting.host.as_str(), hosting.path.as_str());
        let endpoint = format!("repos/{path}/commits?per_page=100");
        let mut args = vec!["api", endpoint.as_str()];
        if host != "github.com" {
            args.extend(["--hostname", host]);
        }
        let json = gh(repo, &args).or_else(|e| {
            if host != "github.com" {
                return Err(e);
            }
            ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("gitcraft")
                .build()
                .get(&format!("https://api.github.com/{endpoint}"))
                .call()
                .map_err(|e| e.to_string())?
                .into_string()
                .map_err(|e| e.to_string())
        });
        json.map(|j| super::parse_commit_avatars(&j))
            .unwrap_or_default()
            .into_iter()
            .map(|(email, url)| (email, Hint::Url(url)))
            .collect()
    }

    /// Two lists: those whose review is asked of you (or a team of yours), and yours with
    /// their checks, comments and reviews.
    fn inbox(&self, repo: &Path, _: &Hosting) -> Result<inbox::Answer, String> {
        const FIELDS: &str = "number,title,url,author,isDraft,updatedAt";
        const MAX: usize = 50;
        let list = |filter: &[&str], fields: &str| {
            let max = MAX.to_string();
            let mut args = vec!["pr", "list", "--state", "open", "--limit", &max];
            args.extend(filter);
            args.extend(["--json", fields]);
            cli::run_within(TOOL, NOUN, repo, &args, inbox::TOOL_FOR)
        };
        let asked =
            inbox::parse_github(&list(&["--search", "review-requested:@me"], FIELDS)?, true)?;
        let mine_fields = format!("{FIELDS},statusCheckRollup,comments,reviews");
        let mine = inbox::parse_github(&list(&["--author", "@me"], &mine_fields)?, false)?;
        let cut = [
            inbox::cut_note(asked.len(), MAX, "asking your review"),
            inbox::cut_note(mine.len(), MAX, "of yours"),
        ];
        Ok(inbox::Answer {
            prs: inbox::merged(vec![asked, mine]),
            cut: cut.into_iter().flatten().collect(),
        })
    }

    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
    }
}

impl Pages for GitHub {
    /// `gh` finds the repository from the remotes itself, as `gh pr view` in a terminal would.
    fn details(&self, repo: &Path, _: &Hosting, number: u64) -> Result<PrDetails, String> {
        details::details(repo, number)
    }

    fn target(&self, repo: &Path, _: &Hosting, details: &PrDetails) -> Option<Box<dyn Target>> {
        post::GitHubTarget::of(repo, details).map(|t| Box::new(t) as Box<dyn Target>)
    }
}

fn parse_list(json: &str) -> Result<Vec<PullRequest>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected {TOOL} output: {e}"))?;
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| PullRequest {
            number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
            title: at(item, "/title").to_owned(),
            branch: at(item, "/headRefName").to_owned(),
            sha: at(item, "/headRefOid").to_owned(),
            author: login(item, "/author/login"),
            url: at(item, "/url").to_owned(),
            draft: item
                .get("isDraft")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_goes_by_its_name_alone() {
        let v: Value = serde_json::json!({"author": {"login": "app/github-actions"}});
        assert_eq!(login(&v, "/author/login"), "github-actions");
        let person: Value = serde_json::json!({"author": {"login": "ana"}});
        assert_eq!(login(&person, "/author/login"), "ana");
    }

    #[test]
    fn an_app_opening_a_pull_request_goes_by_its_name_alone() {
        let json = r#"[{"number": 1, "title": "t", "headRefName": "b",
            "author": {"login": "app/github-actions"}, "url": "u", "isDraft": false}]"#;
        let list = parse_list(json);
        assert_eq!(
            list.map(|l| l[0].author.clone()).as_deref(),
            Ok("github-actions")
        );
    }

    #[test]
    fn parses_pull_requests() {
        let gh = r#"[{"number":7,"title":"Fix","headRefName":"fix/x","headRefOid":"c0ffee","author":{"login":"ana"},"url":"https://g/7","isDraft":true}]"#;
        let prs = parse_list(gh).unwrap_or_default();
        assert_eq!(prs.len(), 1);
        assert_eq!(
            (prs[0].number, prs[0].branch.as_str(), prs[0].draft),
            (7, "fix/x", true)
        );
        assert_eq!(prs[0].sha, "c0ffee");
        assert!(parse_list("oops").is_err());
    }
}
