//! Bitbucket Cloud native PR pages and explicit review; merge stays on the forge.
mod details;
mod post;
#[cfg(test)]
mod tests;
use super::{
    cloud::{Api, Service, required, segment, text},
    *,
};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};
pub(super) struct Bitbucket;
fn endpoint(h: &Hosting) -> Result<String, String> {
    let parts = h.path.split('/').collect::<Vec<_>>();
    if h.forge != Forge::Bitbucket
        || h.host != "bitbucket.org"
        || h.web != "https://bitbucket.org"
        || parts.len() != 2
        || parts.iter().any(|p| !super::cloud::plain(p))
    {
        return Err("Use a hosted Bitbucket Cloud workspace/repository remote".into());
    }
    Ok(format!(
        "repositories/{}/{}",
        segment(parts[0]),
        segment(parts[1])
    ))
}
fn url(h: &Hosting, n: u64) -> String {
    format!(
        "https://bitbucket.org/{}/pull-requests/{n}",
        crate::pr_post::encode_path(&h.path)
    )
}
fn repository(api: &Api) -> Result<Value, String> {
    let v = api.get(&endpoint(&api.hosting)?)?;
    if !required(&v, "/full_name")?.eq_ignore_ascii_case(&api.hosting.path)
        || required(&v, "/uuid")?.is_empty()
    {
        return Err("Bitbucket returned a different repository".into());
    }
    Ok(v)
}
/// Follow only the same native collection; never credentials to a returned foreign next URL.
fn pages(api: &Api, first: &str) -> Result<Vec<Value>, String> {
    let collection = first.split('?').next().ok_or("Missing collection")?;
    let expected = format!("https://api.bitbucket.org/2.0/{collection}");
    let mut path = first.to_owned();
    let mut seen = HashSet::new();
    let mut out = vec![];
    loop {
        if !seen.insert(path.clone()) {
            return Err("Bitbucket repeated a page cursor".into());
        }
        let v = api.get(&path)?;
        out.extend(
            v.get("values")
                .and_then(Value::as_array)
                .cloned()
                .ok_or("Bitbucket returned no result list")?,
        );
        if out.len() > 10000 || seen.len() > 100 {
            return Err(
                "Bitbucket collection exceeds the supported bound; use its forge page".into(),
            );
        }
        let next = text(&v, "/next");
        if next.is_empty() {
            return Ok(out);
        }
        let u = url::Url::parse(&next).map_err(|_| "Bitbucket returned an invalid next URL")?;
        if !u.username().is_empty()
            || u.password().is_some()
            || u.fragment().is_some()
            || u.port().is_some_and(|p| p != 443)
            || format!(
                "{}://{}{}",
                u.scheme(),
                u.host_str().unwrap_or_default(),
                u.path()
            ) != expected
        {
            return Err("Bitbucket pagination left the displayed collection".into());
        }
        path = format!(
            "{collection}?{}",
            u.query().ok_or("Bitbucket next page has no cursor")?
        );
    }
}
fn parse(v: &Value, h: &Hosting) -> Result<PullRequest, String> {
    let n = v
        .get("id")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or("Bitbucket PR has no native ID")?;
    Ok(PullRequest {
        number: n,
        title: required(v, "/title")?.into(),
        branch: required(v, "/source/branch/name")?.into(),
        base: required(v, "/destination/branch/name")?.into(),
        sha: required(v, "/source/commit/hash")?.into(),
        from_fork: text(v, "/source/repository/uuid") != text(v, "/destination/repository/uuid"),
        author: text(v, "/author/display_name"),
        url: url(h, n),
        draft: v.get("draft").and_then(Value::as_bool).unwrap_or(false),
    })
}
fn create(api: &Api, n: &NewPullRequest) -> Result<String, String> {
    if !crate::pull_request::is_branch_name(n.branch)
        || !crate::pull_request::is_branch_name(n.base)
        || n.title.trim().is_empty()
    {
        return Err("Choose valid source/target branches and a title".into());
    }
    let repo = repository(api)?;
    api.guard_context()?;
    let reviewers = n
        .reviewers
        .iter()
        .map(|id| json!({"uuid":id}))
        .collect::<Vec<_>>();
    if n.reviewers
        .iter()
        .any(|s| s.is_empty() || s.chars().any(char::is_control))
    {
        return Err("Bitbucket reviewers must be native UUIDs".into());
    }
    let v=api.send("POST",&format!("{}/pullrequests",endpoint(&api.hosting)?),Some(&json!({"title":n.title,"description":n.body,"draft":n.draft,"source":{"branch":{"name":n.branch},"repository":{"uuid":repo["uuid"]}},"destination":{"branch":{"name":n.base}},"reviewers":reviewers})))?;
    parse(&v, &api.hosting)
        .map(|p| p.url)
        .map_err(|e| format!("Create response unreadable; refresh before creating again: {e}"))
}
impl Provider for Bitbucket {
    fn name(&self) -> &'static str {
        "Bitbucket Cloud"
    }
    fn claims(&self, h: &str) -> bool {
        h == "bitbucket.org"
    }
    fn list(&self, _: &Path, h: &Hosting) -> Result<Vec<PullRequest>, String> {
        let api = Api::new(Service::Bitbucket, h)?;
        repository(&api)?;
        pages(
            &api,
            &format!("{}/pullrequests?state=OPEN&pagelen=100", endpoint(h)?),
        )?
        .iter()
        .map(|v| parse(v, h))
        .collect()
    }
    fn create(&self, _: &Path, h: &Hosting, n: &NewPullRequest) -> Result<String, String> {
        create(&Api::new(Service::Bitbucket, h)?, n)
    }
    fn retarget(&self, _: &Path, h: &Hosting, n: u64, base: &str) -> Result<(), String> {
        if !crate::pull_request::is_branch_name(base) {
            return Err("Invalid target branch".into());
        }
        let api = Api::new(Service::Bitbucket, h)?;
        details::snapshot(&api, n)?;
        api.guard_context()?;
        api.send(
            "PUT",
            &format!("{}/pullrequests/{n}", endpoint(h)?),
            Some(&json!({"destination":{"branch":{"name":base}}})),
        )
        .map(|_| ())
    }
    fn find(&self, _: &Path, h: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        let api = Api::new(Service::Bitbucket, h)?;
        let repo = repository(&api)?;
        let q = format!(
            "source.branch.name={} AND source.repository.uuid={}",
            json!(branch),
            repo["uuid"]
        );
        let rows = pages(
            &api,
            &format!(
                "{}/pullrequests?state=OPEN&state=MERGED&state=DECLINED&state=SUPERSEDED&pagelen=100&q={}",
                endpoint(h)?,
                segment(&q)
            ),
        )?;
        let mut found = vec![];
        for v in rows {
            let pr = parse(&v, h)?;
            if pr.branch == branch && !pr.from_fork {
                found.push(Found {
                    pr,
                    state: match text(&v, "/state").as_str() {
                        "OPEN" => PrState::Open,
                        "MERGED" => PrState::Merged,
                        _ => PrState::Closed,
                    },
                });
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
impl Pages for Bitbucket {
    fn details(&self, _: &Path, h: &Hosting, n: u64) -> Result<PrDetails, String> {
        details::read(&Api::new(Service::Bitbucket, h)?, n)
    }
    fn target(&self, _: &Path, h: &Hosting, d: &PrDetails) -> Option<Box<dyn Target>> {
        if d.forge != Some(Forge::Bitbucket)
            || d.url != url(h, d.number)
            || d.extras.connection_binding != Some(crate::pr_threads::connection_binding())
        {
            return None;
        }
        Some(Box::new(post::BitbucketTarget::new(
            Api::new(Service::Bitbucket, h).ok()?,
            d,
        )?))
    }
}
