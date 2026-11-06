//! Where the open pull requests come from: a fork or the repository itself, whether the author
//! lets maintainers push to their branch, whether they are new to the repository, and (on
//! GitHub) the workflow runs of their commits waiting for a maintainer's approval. Each forge
//! says it its own way; this reads it into [`ForkInfo`].

use serde_json::Value;

use crate::pull_request::at;

/// Where one pull request's branch is, and what its author allows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ForkInfo {
    pub number: u64,
    /// The branch is in another repository than the one it is proposed to (a fork).
    pub cross: bool,
    /// The fork's owner (`ann`), as a name for its remote.
    pub owner: String,
    /// The fork, `ann/app` (on GitLab, with its namespace).
    pub repo_path: String,
    /// Its clone URLs. GitLab's list doesn't give them: filled in by asking for the project.
    pub clone_https: String,
    pub clone_ssh: String,
    /// The branch in the fork.
    pub branch: String,
    /// The branch it is proposed to (`main`).
    pub base: String,
    /// Its latest commit.
    pub sha: String,
    /// The author lets maintainers push to the branch.
    pub maintainers_can_edit: bool,
    /// The author's first contribution to the repository.
    pub first_time: bool,
    /// GitLab: the fork's project id, to ask for its clone URLs.
    pub project_id: u64,
}

fn number(v: &Value, pointer: &str) -> u64 {
    v.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

fn flag(v: &Value, pointer: &str) -> bool {
    v.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

fn items(json: &str) -> Result<Vec<Value>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

/// GitHub's REST list of pull requests (`repos/o/r/pulls`).
pub(crate) fn parse_github(json: &str) -> Result<Vec<ForkInfo>, String> {
    Ok(items(json)?.iter().map(github_one).collect())
}

fn github_one(pr: &Value) -> ForkInfo {
    let head_repo = at(pr, "/head/repo/full_name");
    let base_repo = at(pr, "/base/repo/full_name");
    ForkInfo {
        number: number(pr, "/number"),
        // A fork deleted since has no repository: still not the base's.
        cross: head_repo.is_empty() || !head_repo.eq_ignore_ascii_case(base_repo),
        owner: at(pr, "/head/repo/owner/login").to_owned(),
        repo_path: head_repo.to_owned(),
        clone_https: at(pr, "/head/repo/clone_url").to_owned(),
        clone_ssh: at(pr, "/head/repo/ssh_url").to_owned(),
        branch: at(pr, "/head/ref").to_owned(),
        base: at(pr, "/base/ref").to_owned(),
        sha: at(pr, "/head/sha").to_owned(),
        maintainers_can_edit: flag(pr, "/maintainer_can_modify"),
        first_time: matches!(
            at(pr, "/author_association"),
            "FIRST_TIME_CONTRIBUTOR" | "FIRST_TIMER"
        ),
        project_id: 0,
    }
}

/// The workflow runs waiting for approval (GitHub's `actions/runs?status=action_required`):
/// `(commit, run id)`.
pub(crate) fn parse_waiting_runs(json: &str) -> Result<Vec<(String, u64)>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    Ok(v.get("workflow_runs")
        .and_then(Value::as_array)
        .map(|runs| {
            runs.iter()
                .map(|r| (at(r, "/head_sha").to_owned(), number(r, "/id")))
                .filter(|(sha, id)| !sha.is_empty() && *id > 0)
                .collect()
        })
        .unwrap_or_default())
}

/// GitLab's list of merge requests (`projects/:id/merge_requests`). The fork's clone URLs come
/// from [`fill_gitlab_project`].
pub(crate) fn parse_gitlab(json: &str) -> Result<Vec<ForkInfo>, String> {
    Ok(items(json)?
        .iter()
        .map(|mr| {
            let source = number(mr, "/source_project_id");
            ForkInfo {
                number: number(mr, "/iid"),
                cross: source != number(mr, "/target_project_id"),
                owner: at(mr, "/author/username").to_owned(),
                branch: at(mr, "/source_branch").to_owned(),
                base: at(mr, "/target_branch").to_owned(),
                sha: at(mr, "/sha").to_owned(),
                maintainers_can_edit: flag(mr, "/allow_collaboration")
                    || flag(mr, "/allow_maintainer_to_push"),
                first_time: flag(mr, "/first_contribution"),
                project_id: source,
                ..ForkInfo::default()
            }
        })
        .collect())
}

/// Fills in the fork's place and clone URLs from GitLab's project (`projects/:id`).
pub(crate) fn fill_gitlab_project(info: &mut ForkInfo, json: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unexpected answer: {e}"))?;
    info.repo_path = at(&v, "/path_with_namespace").to_owned();
    info.clone_https = at(&v, "/http_url_to_repo").to_owned();
    info.clone_ssh = at(&v, "/ssh_url_to_repo").to_owned();
    let namespace = at(&v, "/namespace/path");
    if !namespace.is_empty() {
        info.owner = namespace.to_owned();
    }
    Ok(())
}

/// Forgejo's and Gitea's list of pull requests (`repos/o/r/pulls`), one item.
pub(crate) fn gitea_one(pr: &Value) -> ForkInfo {
    let head = number(pr, "/head/repo_id");
    let base = number(pr, "/base/repo_id");
    ForkInfo {
        number: number(pr, "/number"),
        cross: head != base || head == 0,
        owner: at(pr, "/head/repo/owner/login").to_owned(),
        repo_path: at(pr, "/head/repo/full_name").to_owned(),
        clone_https: at(pr, "/head/repo/clone_url").to_owned(),
        clone_ssh: at(pr, "/head/repo/ssh_url").to_owned(),
        branch: at(pr, "/head/ref").to_owned(),
        base: at(pr, "/base/ref").to_owned(),
        sha: at(pr, "/head/sha").to_owned(),
        maintainers_can_edit: flag(pr, "/allow_maintainer_edit"),
        first_time: false,
        project_id: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_pull_requests_from_forks_and_from_the_repository() {
        let json = r#"[
          {"number": 7, "author_association": "FIRST_TIME_CONTRIBUTOR",
           "maintainer_can_modify": true,
           "head": {"ref": "fix/ü", "sha": "abc",
                    "repo": {"full_name": "ann/app", "owner": {"login": "ann"},
                             "clone_url": "https://github.com/ann/app.git",
                             "ssh_url": "git@github.com:ann/app.git"}},
           "base": {"repo": {"full_name": "acme/app"}}},
          {"number": 8, "author_association": "MEMBER", "maintainer_can_modify": false,
           "head": {"ref": "feat", "sha": "def",
                    "repo": {"full_name": "acme/app", "owner": {"login": "acme"}}},
           "base": {"repo": {"full_name": "acme/app"}}},
          {"number": 9, "head": {"ref": "gone", "sha": "x", "repo": null},
           "base": {"repo": {"full_name": "acme/app"}}}
        ]"#;
        let list = parse_github(json).unwrap_or_default();
        assert_eq!(list.len(), 3);
        assert!(list[0].cross && list[0].maintainers_can_edit && list[0].first_time);
        assert_eq!(list[0].owner, "ann");
        assert_eq!(list[0].branch, "fix/ü");
        assert_eq!(list[0].clone_ssh, "git@github.com:ann/app.git");
        assert!(!list[1].cross && !list[1].first_time);
        assert!(list[2].cross, "a deleted fork");
        assert!(parse_github("not json").is_err());
    }

    #[test]
    fn github_runs_waiting_for_approval() {
        let json = r#"{"workflow_runs": [{"id": 11, "head_sha": "abc"}, {"id": 0}]}"#;
        assert_eq!(
            parse_waiting_runs(json).unwrap_or_default(),
            [("abc".to_owned(), 11)]
        );
        assert_eq!(parse_waiting_runs("{}").unwrap_or_default(), []);
    }

    #[test]
    fn gitlab_merge_requests_and_their_fork_project() {
        let json = r#"[{"iid": 3, "source_project_id": 20, "target_project_id": 10,
            "source_branch": "fix", "sha": "abc", "allow_collaboration": true,
            "author": {"username": "ann"}, "first_contribution": true}]"#;
        let mut list = parse_gitlab(json).unwrap_or_default();
        assert!(list[0].cross && list[0].maintainers_can_edit && list[0].first_time);
        assert_eq!(list[0].project_id, 20);
        let project = r#"{"path_with_namespace": "ann-group/app",
            "http_url_to_repo": "https://gitlab.com/ann-group/app.git",
            "ssh_url_to_repo": "git@gitlab.com:ann-group/app.git",
            "namespace": {"path": "ann-group"}}"#;
        assert!(fill_gitlab_project(&mut list[0], project).is_ok());
        assert_eq!(list[0].owner, "ann-group");
        assert_eq!(list[0].clone_https, "https://gitlab.com/ann-group/app.git");
    }

    #[test]
    fn gitea_pull_requests() {
        let pr: Value = serde_json::from_str(
            r#"{"number": 2, "allow_maintainer_edit": true,
               "head": {"ref": "fix", "sha": "abc", "repo_id": 5,
                        "repo": {"full_name": "reviewer/app", "owner": {"login": "reviewer"},
                                 "clone_url": "http://127.0.0.1:3000/reviewer/app.git",
                                 "ssh_url": ""}},
               "base": {"repo_id": 4}}"#,
        )
        .unwrap_or_default();
        let info = gitea_one(&pr);
        assert!(info.cross && info.maintainers_can_edit);
        assert_eq!(info.owner, "reviewer");
        assert_eq!(info.repo_path, "reviewer/app");
    }
}
