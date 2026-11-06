//! GitLab, through `glab`: the list of merge requests and opening one. Their page is the web's.

use std::path::Path;

use serde_json::Value;

use super::inbox;
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
