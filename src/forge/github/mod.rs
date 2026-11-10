//! GitHub, through `gh`: the list of pull requests, opening one, and the page of one in full —
//! reading it ([`details`], [`threads`]) and acting on it ([`post`]); the issues assigned to the
//! user ([`issues`]).

mod details;
mod issues;
mod post;
mod threads;

use std::path::Path;

use serde_json::Value;

use super::inbox;
use super::{
    ForkInfo, Found, Hosting, Issue, MergedPullRequest, NewPullRequest, Pages, PrState, Provider,
    PullRequest, cli,
};
use crate::avatars::Hint;
use crate::pr_post::Target;
use crate::pull_request::{PrDetails, at};

pub(super) struct GitHub;

const TOOL: &str = "gh";
const NOUN: &str = "pull request";

fn gh(repo: &Path, hosting: &Hosting, args: &[&str]) -> Result<String, String> {
    cli::run(TOOL, NOUN, repo, hosting, args)
}

/// `gh api …` about the repository's host (GitHub Enterprise named with `--hostname`).
fn api(repo: &Path, hosting: &Hosting, args: &[&str]) -> Result<String, String> {
    gh(repo, hosting, args)
}

/// [`gh`], with `input` on its stdin.
fn gh_with_input(
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, String> {
    cli::run_with_input(TOOL, NOUN, repo, hosting, args, input)
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

    fn list(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
        let out = gh(
            repo,
            hosting,
            &[
                "pr",
                "list",
                "--state",
                "open",
                "--limit",
                "100",
                "--json",
                "number,title,headRefName,baseRefName,baseRefOid,headRefOid,author,url,isDraft,isCrossRepository",
            ],
        )?;
        parse_list(&out)
    }

    fn merged(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        let args = [
            "pr",
            "list",
            "--state",
            "merged",
            "--limit",
            "100",
            "--json",
            "number,headRefName,headRefOid,baseRefName",
        ];
        let pointers = ["/number", "/headRefName", "/headRefOid", "/baseRefName"];
        super::parse_merged(&gh(repo, hosting, &args)?, pointers, TOOL)
    }

    fn create(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &NewPullRequest,
    ) -> Result<String, String> {
        let mut args = vec![
            "pr", "create", "--head", new.branch, "--base", new.base, "--title", new.title,
            "--body", new.body,
        ];
        if new.draft {
            args.push("--draft");
        }
        let url = gh(repo, hosting, &args)?.trim().to_owned();
        // A URL argument to `gh pr edit` overrides --repo. Validate the answer and edit by
        // number so reviewer requests remain bound to the selected repository as well.
        let number = super::created_pr_number(hosting, &url)?;
        super::ask_reviewers(url.clone(), new.reviewers, |who| {
            gh(
                repo,
                hosting,
                &["pr", "edit", &number, "--add-reviewer", who],
            )
            .map(|_| ())
        })
    }

    fn assigned_issues(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<Issue>, String> {
        issues::assigned(repo, hosting)
    }

    fn issue(&self, repo: &Path, hosting: &Hosting, number: u64) -> Result<Issue, String> {
        issues::one(repo, hosting, number)
    }

    fn forks(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<ForkInfo>, String> {
        let fields = "number,isCrossRepository,maintainerCanModify,headRepositoryOwner,\
                      headRepository,headRefName,headRefOid,baseRefName";
        let out = gh(
            repo,
            hosting,
            &[
                "pr", "list", "--state", "open", "--limit", "100", "--json", fields,
            ],
        )?;
        let mut list = super::forks::parse_github_list(&out, &hosting.host)?;
        // Who is new to the repository is only in the REST list; without it, nobody is marked.
        let endpoint = format!("repos/{}/pulls?state=open&per_page=100", hosting.path);
        if let Ok(rest) = api(repo, hosting, &["api", &endpoint])
            && let Ok(first) = super::forks::github_first_timers(&rest)
        {
            for info in &mut list {
                info.first_time = first.contains(&info.number);
            }
        }
        Ok(list)
    }

    fn waiting_runs(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<(String, u64)>, String> {
        let endpoint = format!(
            "repos/{}/actions/runs?status=action_required&per_page=100",
            hosting.path
        );
        super::forks::parse_waiting_runs(&api(repo, hosting, &["api", &endpoint])?)
    }

    fn approve_run(&self, repo: &Path, hosting: &Hosting, run: u64) -> Result<(), String> {
        let endpoint = format!("repos/{}/actions/runs/{run}/approve", hosting.path);
        api(repo, hosting, &["api", "-X", "POST", &endpoint]).map(|_| ())
    }

    fn retarget(
        &self,
        repo: &Path,
        hosting: &Hosting,
        number: u64,
        base: &str,
    ) -> Result<(), String> {
        let number = number.to_string();
        gh(repo, hosting, &["pr", "edit", &number, "--base", base]).map(drop)
    }

    fn find(&self, repo: &Path, hosting: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        let out = gh(
            repo,
            hosting,
            &[
                "pr",
                "list",
                "--head",
                branch,
                "--state",
                "all",
                "--limit",
                "20",
                "--json",
                "number,title,headRefName,baseRefName,headRefOid,author,url,isDraft,isCrossRepository,state",
            ],
        )?;
        let states: Vec<PrState> = serde_json::from_str::<Value>(&out)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|item| match at(item, "/state") {
                "OPEN" => PrState::Open,
                "MERGED" => PrState::Merged,
                _ => PrState::Closed,
            })
            .collect();
        let found = parse_list(&out)?
            .into_iter()
            .zip(states)
            .filter(|(pr, _)| pr.branch == branch)
            .map(|(pr, state)| Found { pr, state })
            .collect();
        Ok(super::pick_found(found))
    }

    /// `gh release create`, the notes on its stdin; `--verify-tag`: the tag must be pushed.
    fn create_release(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &super::NewRelease,
    ) -> Result<String, String> {
        let mut args = vec![
            "release",
            "create",
            new.tag,
            "--verify-tag",
            "--title",
            new.title,
            "--notes-file",
            "-",
        ];
        if new.prerelease {
            args.push("--prerelease");
        }
        // Made already (a step that timed out after the forge made it, or a workflow of the
        // tag's): that one.
        if let Ok(url) = gh(
            repo,
            hosting,
            &["release", "view", new.tag, "--json", "url", "-q", ".url"],
        ) && !url.trim().is_empty()
        {
            return Ok(url.trim().to_owned());
        }
        gh_with_input(repo, hosting, &args, Some(new.notes)).map(|out| out.trim().to_owned())
    }

    fn upload_assets(
        &self,
        repo: &Path,
        hosting: &Hosting,
        tag: &str,
        files: &[std::path::PathBuf],
    ) -> Result<(), String> {
        let paths = super::path_args(files);
        let mut args = vec!["release", "upload", tag, "--clobber", "--"];
        args.extend(paths.iter().map(String::as_str));
        gh(repo, hosting, &args).map(|_| ())
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
        let json = gh(repo, hosting, &args).or_else(|e| {
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
    fn inbox(&self, repo: &Path, hosting: &Hosting) -> Result<inbox::Answer, String> {
        const FIELDS: &str = "number,title,url,author,isDraft,updatedAt";
        const MAX: usize = 50;
        let list = |filter: &[&str], fields: &str| {
            let max = MAX.to_string();
            let mut args = vec!["pr", "list", "--state", "open", "--limit", &max];
            args.extend(filter);
            args.extend(["--json", fields]);
            cli::run_within(TOOL, NOUN, repo, hosting, &args, inbox::TOOL_FOR)
        };
        let asked =
            inbox::parse_github(&list(&["--search", "review-requested:@me"], FIELDS)?, true)?;
        let mine_fields = format!("{FIELDS},statusCheckRollup,comments,reviews");
        let mine = inbox::parse_github(&list(&["--author", "@me"], &mine_fields)?, false)?;
        let cut = [
            inbox::cut_note(
                asked.len(),
                MAX,
                crate::i18n::text("backend-asking-your-review-3c3907"),
            ),
            inbox::cut_note(
                mine.len(),
                MAX,
                crate::i18n::text("backend-of-yours-2a649f"),
            ),
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
    /// Read from the selected forge repository, even when origin points elsewhere.
    fn details(&self, repo: &Path, hosting: &Hosting, number: u64) -> Result<PrDetails, String> {
        details::details(repo, hosting, number)
    }

    fn target(
        &self,
        repo: &Path,
        hosting: &Hosting,
        details: &PrDetails,
    ) -> Option<Box<dyn Target>> {
        post::GitHubTarget::of(repo, hosting, details).map(|t| Box::new(t) as Box<dyn Target>)
    }
}

fn parse_list(json: &str) -> Result<Vec<PullRequest>, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| {
        crate::i18n::message(
            "backend-unexpected-tool-output-e-164ff7",
            &[
                ("tool", (TOOL).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| PullRequest {
            number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
            title: at(item, "/title").to_owned(),
            branch: at(item, "/headRefName").to_owned(),
            base: at(item, "/baseRefName").to_owned(),
            base_oid: at(item, "/baseRefOid").to_owned(),
            from_fork: item
                .get("isCrossRepository")
                .and_then(Value::as_bool)
                .unwrap_or(false),
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
        let gh = r#"[{"number":7,"title":"Fix","headRefName":"fix/x","baseRefName":"main","baseRefOid":"b0","headRefOid":"c0ffee","author":{"login":"ana"},"url":"https://g/7","isDraft":true}]"#;
        let prs = parse_list(gh).unwrap_or_default();
        assert_eq!(prs.len(), 1);
        assert_eq!(
            (prs[0].number, prs[0].branch.as_str(), prs[0].draft),
            (7, "fix/x", true)
        );
        assert_eq!(prs[0].sha, "c0ffee");
        assert_eq!(prs[0].base, "main");
        assert_eq!(prs[0].base_oid, "b0");
        let mut old_cache = serde_json::to_value(&prs[0]).unwrap();
        old_cache.as_object_mut().unwrap().remove("base_oid");
        let restored: PullRequest = serde_json::from_value(old_cache).unwrap();
        assert!(restored.base_oid.is_empty());
        assert!(parse_list("oops").is_err());
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn provider_boundary_selected_fork_is_explicit_for_github_lists() -> Result<(), String> {
        let dir = std::env::temp_dir();
        let hosting = Hosting {
            forge: super::super::Forge::GitHub,
            host: "github.com".into(),
            path: "fork/project".into(),
            web: "https://github.com".into(),
        };
        let calls = cli::recording::run(vec![Ok("[]".into())], || {
            assert!(GitHub.list(&dir, &hosting)?.is_empty());
            Ok(())
        })?;
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0]
                .args
                .iter()
                .any(|a| a.split(',').any(|f| f == "baseRefOid"))
        );
        assert!(
            calls[0]
                .args
                .windows(2)
                .any(|p| p == ["--repo", "github.com/fork/project"]),
            "selected fork absent from prepared command: {:?}",
            calls[0]
        );
        Ok(())
    }
}
