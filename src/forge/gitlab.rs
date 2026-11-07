//! GitLab, through `glab`: the list of merge requests and opening one (their page is the web's),
//! and the issues assigned to the user.

use std::path::Path;

use serde_json::Value;

use super::inbox;
use super::{ForkInfo, Hosting, Issue, MergedPullRequest, NewPullRequest, Provider, PullRequest, cli};
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
        let out = self.run(repo, &args)?;
        let url = out
            .split_whitespace()
            .find(|w| w.starts_with("http"))
            .unwrap_or(out.trim())
            .to_owned();
        let Some(iid) = url.rsplit('/').next().filter(|n| n.parse::<u64>().is_ok()) else {
            return Ok(url);
        };
        let iid = iid.to_owned();
        // Asked after opening it, one at a time (accounts only: glab asks no group).
        let accounts = gitlab_reviewers(new.reviewers);
        super::ask_reviewers(url, &accounts, |who| {
            self.run(
                repo,
                &["mr", "update", &iid, "--reviewer", &format!("+{who}")],
            )
            .map(|_| ())
        })
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

    /// Two lists: those whose review is asked of you, and yours. The list has neither
    /// pipelines nor who wrote the notes, so each of yours (the first few) is read once more,
    /// with its notes.
    fn inbox(&self, repo: &Path, hosting: &Hosting) -> Result<inbox::Answer, String> {
        const MAX: usize = 50;
        let run = |args: &[&str]| cli::run_within(TOOL, self.noun(), repo, args, inbox::TOOL_FOR);
        let list = |who: &str| {
            let max = MAX.to_string();
            run(&["mr", "list", who, "--output", "json", "--per-page", &max])
        };
        let asked = inbox::parse_gitlab(&list("--reviewer=@me")?, true)?;
        let mut mine = inbox::parse_gitlab(&list("--author=@me")?, false)?;
        let mut cut: Vec<String> = [
            inbox::cut_note(asked.len(), MAX, "asking your review"),
            inbox::cut_note(mine.len(), MAX, "of yours"),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Named by its path, as `glab api` takes it wherever it runs: `:id` is the project of
        // the folder glab runs in, which for a repository on another machine is none.
        let project = project_id(&hosting.path)
            .ok_or_else(|| format!("{TOOL} needs a plain group/project path"))?;
        let host = api_authority(hosting);
        let api = |rest: String| run(&["api", "--hostname", &host, &rest]);
        if mine.len() > inbox::MAX_MINE {
            cut.push(format!(
                "the pipelines and notes of only {} of your {} merge requests were read",
                inbox::MAX_MINE,
                mine.len()
            ));
        }
        for mr in mine.iter_mut().take(inbox::MAX_MINE) {
            let at = format!("projects/{project}/merge_requests/{}", mr.number);
            if let Ok(json) = api(at.clone()) {
                mr.checks = inbox::gitlab_pipeline(&json);
            }
            if let Ok(json) = api(format!("{at}/notes?sort=desc&per_page=100")) {
                mr.said = inbox::gitlab_notes(&json, &mr.author);
            }
        }
        Ok(inbox::Answer {
            prs: inbox::merged(vec![asked, mine]),
            cut,
        })
    }

    /// The open merge requests, and for those from a fork, the fork's clone URLs (one request
    /// per fork project).
    fn forks(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<ForkInfo>, String> {
        let project = hosting.path.replace('/', "%2F");
        let host = api_authority(hosting);
        let api = |endpoint: &str| self.run(repo, &["api", "--hostname", &host, endpoint]);
        let mut list = super::forks::parse_gitlab(&api(&format!(
            "projects/{project}/merge_requests?state=opened&per_page=100"
        ))?)?;
        // The fork's clone URLs only for those a maintainer can push to: one request per fork
        // project, and one that fails leaves that one without (Check out then says so).
        let mut asked: Vec<(u64, Option<String>)> = Vec::new();
        for info in list
            .iter_mut()
            .filter(|i| i.cross && i.maintainers_can_edit && i.project_id > 0)
        {
            let id = info.project_id;
            if !asked.iter().any(|(p, _)| *p == id) {
                asked.push((id, api(&format!("projects/{id}")).ok()));
            }
            if let Some((_, Some(json))) = asked.iter().find(|(p, _)| *p == id) {
                let _ = super::forks::fill_gitlab_project(info, json);
            }
        }
        Ok(list)
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

/// The reviewers `glab` can ask: accounts, not groups (`acme/payments`).
fn gitlab_reviewers(reviewers: &[String]) -> Vec<String> {
    reviewers
        .iter()
        .filter(|r| !r.contains('/'))
        .cloned()
        .collect()
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

/// `group%2Fsub%2Fproject`, how the API names a project by its path; none for a path that
/// isn't plain (it goes into the API's path as it is).
fn project_id(path: &str) -> Option<String> {
    let plain = path.split('/').all(|part| {
        !part.is_empty()
            && !part.starts_with(['-', '.'])
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    });
    (plain && path.contains('/')).then(|| path.replace('/', "%2F"))
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
    fn a_project_is_named_by_its_encoded_path() {
        assert_eq!(project_id("g/sub/p.x").as_deref(), Some("g%2Fsub%2Fp.x"));
        for bad in ["p", "g//p", "g/../p", "g/p?x=1", "-g/p"] {
            assert_eq!(project_id(bad), None, "{bad}");
        }
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
