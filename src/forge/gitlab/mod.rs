//! GitLab, through `glab`: the list of merge requests and opening one (their page is the web's),
//! and the issues assigned to the user.

use std::path::Path;

use serde_json::Value;

use super::inbox;
use super::{
    ForkInfo, Found, Hosting, Issue, MergedPullRequest, NewPullRequest, Pages, PrState, Provider,
    PullRequest, cli,
};
use crate::avatars::Hint;

mod api;
mod details;
#[cfg(test)]
mod native_tests;
mod post;

pub(super) struct GitLab;

const TOOL: &str = "glab";

impl GitLab {
    fn run(&self, repo: &Path, hosting: &Hosting, args: &[&str]) -> Result<String, String> {
        cli::run(TOOL, self.noun(), repo, hosting, args)
    }
}

impl Provider for GitLab {
    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
    }

    fn name(&self) -> &'static str {
        "GitLab"
    }

    fn noun(&self) -> &'static str {
        "merge request"
    }

    fn claims(&self, host: &str) -> bool {
        host.contains("gitlab")
    }

    fn list(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
        // glab gives 30 at a time otherwise; 100, as GitHub's and Forgejo's lists have.
        let args = ["mr", "list", "--output", "json", "--per-page", "100"];
        parse_list(&self.run(repo, hosting, &args)?)
    }

    fn merged(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
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
        super::parse_merged(&self.run(repo, hosting, &args)?, pointers, TOOL)
    }

    fn create(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &NewPullRequest,
    ) -> Result<String, String> {
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
        let out = self.run(repo, hosting, &args)?;
        let url = out
            .split_whitespace()
            .find(|w| w.starts_with("http"))
            .unwrap_or(out.trim())
            .to_owned();
        let iid = super::created_pr_number(hosting, &url)?;
        // Asked after opening it, one at a time (accounts only: glab asks no group).
        let accounts = gitlab_reviewers(new.reviewers);
        super::ask_reviewers(url, &accounts, |who| {
            self.run(
                repo,
                hosting,
                &["mr", "update", &iid, "--reviewer", &format!("+{who}")],
            )
            .map(|_| ())
        })
    }

    /// `@me` is the account `glab` is signed in to for the host.
    fn assigned_issues(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<Issue>, String> {
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
        parse_issues(&cli::run(TOOL, "issue", repo, hosting, &args)?)
    }

    fn issue(&self, repo: &Path, hosting: &Hosting, number: u64) -> Result<Issue, String> {
        let number = number.to_string();
        let args = ["issue", "view", &number, "--output", "json"];
        let out = cli::run(TOOL, "issue", repo, hosting, &args)?;
        let value: Value = serde_json::from_str(&out).map_err(|e| {
            crate::i18n::message(
                "backend-unexpected-tool-output-e-164ff7",
                &[
                    ("tool", (TOOL).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            )
        })?;
        parse_issue(&value).ok_or_else(|| {
            crate::i18n::message(
                "backend-unexpected-tool-output-not-an-issue-ae58a9",
                &[("tool", (TOOL).to_string().into())],
            )
        })
    }

    /// Two lists: those whose review is asked of you, and yours. The list has neither
    /// pipelines nor who wrote the notes, so each of yours (the first few) is read once more,
    /// with its notes.
    fn inbox(&self, repo: &Path, hosting: &Hosting) -> Result<inbox::Answer, String> {
        const MAX: usize = 50;
        let run = |args: &[&str]| {
            cli::run_within(TOOL, self.noun(), repo, hosting, args, inbox::TOOL_FOR)
        };
        let list = |who: &str| {
            let max = MAX.to_string();
            run(&["mr", "list", who, "--output", "json", "--per-page", &max])
        };
        let asked = inbox::parse_gitlab(&list("--reviewer=@me")?, true)?;
        let mut mine = inbox::parse_gitlab(&list("--author=@me")?, false)?;
        let mut cut: Vec<String> = [
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
        ]
        .into_iter()
        .flatten()
        .collect();
        // Named by its path, as `glab api` takes it wherever it runs: `:id` is the project of
        // the folder glab runs in, which for a repository on another machine is none.
        let project = project_id(&hosting.path).ok_or_else(|| {
            crate::i18n::message(
                "backend-tool-needs-a-plain-group-project-path-2c0929",
                &[("tool", (TOOL).to_string().into())],
            )
        })?;
        let host = api_authority(hosting);
        let api = |rest: String| run(&["api", "--hostname", &host, &rest]);
        if mine.len() > inbox::MAX_MINE {
            cut.push(crate::i18n::message(
                "backend-the-pipelines-and-notes-of-only-of-your-merge-requests-were-read-e52d15",
                &[
                    ("inbox", (inbox::MAX_MINE).into()),
                    ("count", (mine.len()).into()),
                ],
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
        let api = |endpoint: &str| self.run(repo, hosting, &["api", "--hostname", &host, endpoint]);
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

    fn retarget(
        &self,
        repo: &Path,
        hosting: &Hosting,
        number: u64,
        base: &str,
    ) -> Result<(), String> {
        let number = number.to_string();
        let args = [
            "mr",
            "update",
            number.as_str(),
            "--target-branch",
            base,
            "--yes",
        ];
        self.run(repo, hosting, &args).map(drop)
    }

    fn find(&self, repo: &Path, hosting: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        let args = [
            "mr",
            "list",
            "--source-branch",
            branch,
            "--all",
            "--output",
            "json",
            "--per-page",
            "20",
        ];
        let out = self.run(repo, hosting, &args)?;
        let states: Vec<PrState> = serde_json::from_str::<Value>(&out)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|item| match item.get("state").and_then(Value::as_str) {
                Some("opened") => PrState::Open,
                Some("merged") => PrState::Merged,
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

    fn create_release(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &super::NewRelease,
    ) -> Result<String, String> {
        let url = format!("{}/{}/-/releases/{}", hosting.web, hosting.path, new.tag);
        // Made already: that one.
        if self
            .run(repo, hosting, &["release", "view", new.tag])
            .is_ok()
        {
            return Ok(url);
        }
        // The notes from a file: a long history would not fit a command line.
        let notes = std::env::temp_dir().join(format!(
            "gitcraft-release-notes-{}-{}.md",
            std::process::id(),
            new.tag.replace(['/', '\\'], "-")
        ));
        std::fs::write(&notes, new.notes).map_err(|e| {
            crate::i18n::message(
                "backend-couldn-t-write-the-notes-e-26c183",
                &[("detail", (e).to_string().into())],
            )
        })?;
        let path = notes.to_string_lossy().into_owned();
        let made = self.run(
            repo,
            hosting,
            &[
                "release",
                "create",
                new.tag,
                "--name",
                new.title,
                "--notes-file",
                &path,
            ],
        );
        let _ = std::fs::remove_file(&notes);
        made.map(|_| url)
    }

    fn upload_assets(
        &self,
        repo: &Path,
        hosting: &Hosting,
        tag: &str,
        files: &[std::path::PathBuf],
    ) -> Result<(), String> {
        // One at a time: one sent already (a resumed upload) is passed over.
        for path in super::path_args(files) {
            match self.run(repo, hosting, &["release", "upload", tag, &path]) {
                Err(e) if e.to_lowercase().contains("already") => {}
                other => {
                    other?;
                }
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests;

mod parsing;
pub(super) use parsing::api_authority;
use parsing::*;
