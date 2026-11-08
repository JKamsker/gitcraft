//! Azure DevOps hosted repositories, native pull-request pages and revision-bound review.
mod details;
mod post;
#[cfg(test)]
mod tests;
use super::{
    cloud::{Api, Service, required, segment, text},
    *,
};
use serde_json::{Value, json};
use std::path::Path;
pub(super) struct Azure;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Location {
    pub org: String,
    pub project: String,
    pub repo: String,
}
impl Location {
    fn path(&self) -> String {
        format!("{}/{}/_git/{}", self.org, self.project, self.repo)
    }
    fn of(h: &Hosting) -> Result<Self, String> {
        let p = h.path.split('/').collect::<Vec<_>>();
        if h.host != "dev.azure.com"
            || h.web != "https://dev.azure.com"
            || p.len() != 4
            || p[2] != "_git"
            || p.iter().any(|p| p.is_empty())
        {
            return Err(crate::i18n::text(
                "backend-use-a-canonical-hosted-azure-organization-project-git-repository-a1513e",
            )
            .into());
        }
        Ok(Self {
            org: p[0].into(),
            project: p[1].into(),
            repo: p[3].into(),
        })
    }
}
fn decode(s: &str) -> Option<String> {
    let mut out = vec![];
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        if b[i] == b'%' {
            let v = u8::from_str_radix(std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?, 16).ok()?;
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
/// Azure's SSH/legacy web routes have the same canonical service identity.
pub(super) fn location(remote: &str) -> Option<(String, String)> {
    let (host, raw) = if let Some(p) = remote.strip_prefix("git@ssh.dev.azure.com:") {
        ("ssh.dev.azure.com".into(), p.into())
    } else {
        let u = url::Url::parse(remote).ok()?;
        if !matches!(u.scheme(), "https" | "ssh")
            || u.port().is_some_and(|p| p != 443 && u.scheme() == "https")
        {
            return None;
        }
        (
            u.host_str()?.to_owned(),
            u.path().trim_matches('/').to_owned(),
        )
    };
    let p = raw.split('/').map(decode).collect::<Option<Vec<_>>>()?;
    let loc = if host == "ssh.dev.azure.com" && p.len() == 4 && p[0] == "v3" {
        Location {
            org: p[1].to_ascii_lowercase(),
            project: p[2].clone(),
            repo: p[3].clone(),
        }
    } else if host == "dev.azure.com" && p.len() == 4 && p[2] == "_git" {
        Location {
            org: p[0].to_ascii_lowercase(),
            project: p[1].clone(),
            repo: p[3].clone(),
        }
    } else if let Some(org) = host.strip_suffix(".visualstudio.com") {
        let p = if p.first().is_some_and(|p| p == "DefaultCollection") {
            &p[1..]
        } else {
            &p[..]
        };
        if p.len() != 3 || p[1] != "_git" {
            return None;
        }
        Location {
            org: org.into(),
            project: p[0].clone(),
            repo: p[2].clone(),
        }
    } else {
        return None;
    };
    if !super::cloud::plain(&loc.org)
        || [&loc.project, &loc.repo]
            .iter()
            .any(|p| p.is_empty() || p.contains('/') || p.chars().any(char::is_control))
    {
        return None;
    }
    Some(("dev.azure.com".into(), loc.path()))
}
#[derive(Clone)]
pub(super) struct Repository {
    pub id: String,
    pub project: String,
    pub endpoint: String,
}
fn repository(api: &Api) -> Result<Repository, String> {
    let loc = Location::of(&api.hosting)?;
    let v = api.get(&format!(
        "{}/_apis/git/repositories/{}",
        segment(&loc.project),
        segment(&loc.repo)
    ))?;
    if !required(&v, "/name")?.eq_ignore_ascii_case(&loc.repo)
        || !required(&v, "/project/name")?.eq_ignore_ascii_case(&loc.project)
    {
        return Err(crate::i18n::text(
            "backend-azure-returned-a-different-repository-project-ca75b6",
        )
        .into());
    }
    let id = required(&v, "/id")?.to_owned();
    let project = required(&v, "/project/id")?.to_owned();
    if !super::cloud::plain(&id) || !super::cloud::plain(&project) {
        return Err(crate::i18n::text(
            "backend-azure-returned-invalid-stable-repository-identities-5fab09",
        )
        .into());
    }
    Ok(Repository {
        endpoint: format!(
            "{}/_apis/git/repositories/{}",
            segment(&project),
            segment(&id)
        ),
        id,
        project,
    })
}
fn pr_url(h: &Hosting, n: u64) -> String {
    format!(
        "https://dev.azure.com/{}/pullrequest/{n}",
        crate::pr_post::encode_path(&h.path)
    )
}
fn parse(v: &Value, h: &Hosting) -> Result<PullRequest, String> {
    let number = v
        .get("pullRequestId")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or(crate::i18n::text(
            "backend-azure-pr-has-no-native-number-d96652",
        ))?;
    Ok(PullRequest {
        number,
        title: required(v, "/title")?.into(),
        branch: required(v, "/sourceRefName")?
            .strip_prefix("refs/heads/")
            .ok_or(crate::i18n::text(
                "backend-azure-source-is-not-a-branch-cd06b8",
            ))?
            .into(),
        base: required(v, "/targetRefName")?
            .strip_prefix("refs/heads/")
            .ok_or(crate::i18n::text(
                "backend-azure-target-is-not-a-branch-d6829b",
            ))?
            .into(),
        from_fork: v.get("forkSource").is_some_and(|f| !f.is_null()),
        sha: required(v, "/lastMergeSourceCommit/commitId")?.into(),
        author: text(v, "/createdBy/displayName"),
        url: pr_url(h, number),
        draft: v.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
    })
}
fn list(api: &Api, repo: &Repository, criteria: &str) -> Result<Vec<Value>, String> {
    let mut out = vec![];
    for skip in (0..1000).step_by(100) {
        let rows = api.array(&format!(
            "{}/pullrequests?{criteria}&$top=100&$skip={skip}",
            repo.endpoint
        ))?;
        let end = rows.len() < 100;
        out.extend(rows);
        if end {
            return Ok(out);
        }
    }
    Err(crate::i18n::text(
        "backend-azure-returned-more-than-1-000-pull-requests-use-the-forge-page-7b17a6",
    )
    .into())
}
fn create(api: &Api, n: &NewPullRequest) -> Result<String, String> {
    if !crate::pull_request::is_branch_name(n.branch)
        || !crate::pull_request::is_branch_name(n.base)
        || n.title.trim().is_empty()
    {
        return Err(crate::i18n::text(
            "backend-choose-valid-source-target-branches-and-a-title-0803b8",
        )
        .into());
    }
    let repo = repository(api)?;
    api.guard_context()?;
    let reviewers = n
        .reviewers
        .iter()
        .map(|id| {
            if super::cloud::plain(id) {
                Ok(json!({"id":id}))
            } else {
                Err(crate::i18n::text(
                    "backend-azure-reviewers-must-be-native-user-ids-dc5078",
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let v=api.send("POST",&format!("{}/pullrequests",repo.endpoint),Some(&json!({"sourceRefName":format!("refs/heads/{}",n.branch),"targetRefName":format!("refs/heads/{}",n.base),"title":n.title,"description":n.body,"isDraft":n.draft,"reviewers":reviewers})))?;
    parse(&v, &api.hosting).map(|p| p.url).map_err(|e| {
        crate::i18n::message(
            "backend-azure-create-response-could-not-be-read-refresh-the-list-before-r-647cf7",
            &[("detail", (e).to_string().into())],
        )
    })
}
impl Provider for Azure {
    fn name(&self) -> &'static str {
        "Azure DevOps"
    }
    fn claims(&self, host: &str) -> bool {
        host == "dev.azure.com"
            || host == "ssh.dev.azure.com"
            || host.ends_with(".visualstudio.com")
    }
    fn list(&self, _: &Path, h: &Hosting) -> Result<Vec<PullRequest>, String> {
        let api = Api::new(Service::Azure, h)?;
        let repo = repository(&api)?;
        list(&api, &repo, "searchCriteria.status=active")?
            .iter()
            .map(|v| parse(v, h))
            .collect()
    }
    fn create(&self, _: &Path, h: &Hosting, n: &NewPullRequest) -> Result<String, String> {
        create(&Api::new(Service::Azure, h)?, n)
    }
    fn retarget(&self, _: &Path, h: &Hosting, number: u64, base: &str) -> Result<(), String> {
        if !crate::pull_request::is_branch_name(base) {
            return Err(crate::i18n::text("backend-invalid-target-branch-d8264c").into());
        }
        let api = Api::new(Service::Azure, h)?;
        let s = details::snapshot(&api, number)?;
        api.guard_context()?;
        api.send(
            "PATCH",
            &s.endpoint(number),
            Some(&json!({"targetRefName":format!("refs/heads/{base}")})),
        )
        .map(|_| ())
    }
    fn find(&self, _: &Path, h: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        let api = Api::new(Service::Azure, h)?;
        let repo = repository(&api)?;
        let criteria = format!(
            "searchCriteria.status=all&searchCriteria.sourceRefName={}",
            segment(&format!("refs/heads/{branch}"))
        );
        let mut found = vec![];
        for v in list(&api, &repo, &criteria)? {
            let pr = parse(&v, h)?;
            if pr.branch == branch && !pr.from_fork {
                let state = match text(&v, "/status").as_str() {
                    "active" => PrState::Open,
                    "completed" => PrState::Merged,
                    _ => PrState::Closed,
                };
                found.push(Found { pr, state });
            }
        }
        Ok(found.into_iter().max_by_key(|f| f.pr.number))
    }
    fn avatars(&self, _: &Path, _: &Hosting, _: &[String]) -> Vec<(String, Hint)> {
        vec![]
    }
    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
    }
}
impl Pages for Azure {
    fn details(&self, _: &Path, h: &Hosting, n: u64) -> Result<PrDetails, String> {
        details::read(&Api::new(Service::Azure, h)?, n)
    }
    fn target(&self, _: &Path, h: &Hosting, d: &PrDetails) -> Option<Box<dyn Target>> {
        if d.forge != Some(Forge::Azure)
            || d.url != pr_url(h, d.number)
            || d.extras.connection_binding != Some(crate::pr_threads::connection_binding())
        {
            return None;
        }
        Some(Box::new(post::AzureTarget::new(
            Api::new(Service::Azure, h).ok()?,
            d,
        )?))
    }
}
