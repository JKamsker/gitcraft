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
        return Err(crate::i18n::text(
            "backend-use-a-hosted-bitbucket-cloud-workspace-repository-remote-91dabd",
        )
        .into());
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
        return Err(
            crate::i18n::text("backend-bitbucket-returned-a-different-repository-a4f546").into(),
        );
    }
    Ok(v)
}
/// Follow only the same native collection; never credentials to a returned foreign next URL.
fn pages(api: &Api, first: &str) -> Result<Vec<Value>, String> {
    let collection = first
        .split('?')
        .next()
        .ok_or(crate::i18n::text("backend-missing-collection-78c679"))?;
    let expected = format!("https://api.bitbucket.org/2.0/{collection}");
    let mut path = first.to_owned();
    let mut seen = HashSet::new();
    let mut out = vec![];
    loop {
        if !seen.insert(path.clone()) {
            return Err(
                crate::i18n::text("backend-bitbucket-repeated-a-page-cursor-a84432").into(),
            );
        }
        let v = api.get(&path)?;
        out.extend(
            v.get("values")
                .and_then(Value::as_array)
                .cloned()
                .ok_or(crate::i18n::text(
                    "backend-bitbucket-returned-no-result-list-7bc9e5",
                ))?,
        );
        if out.len() > 10000 || seen.len() > 100 {
            return Err(crate::i18n::text(
                "backend-bitbucket-collection-exceeds-the-supported-bound-use-its-forge-pa-bce46b",
            )
            .into());
        }
        let next = text(&v, "/next");
        if next.is_empty() {
            return Ok(out);
        }
        let u = url::Url::parse(&next).map_err(|_| {
            crate::i18n::text("backend-bitbucket-returned-an-invalid-next-url-da9652")
        })?;
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
            return Err(crate::i18n::text(
                "backend-bitbucket-pagination-left-the-displayed-collection-4c2d4d",
            )
            .into());
        }
        path = format!(
            "{collection}?{}",
            u.query().ok_or(crate::i18n::text(
                "backend-bitbucket-next-page-has-no-cursor-e07683"
            ))?
        );
    }
}
/// PR diffstat is a 302 convenience route. Use the documented JSON repository
/// endpoint with immutable commits instead, keeping redirects disabled. Bitbucket's
/// spec order is the reverse of git diff: source first, comparison base second.
fn diffstat(api: &Api, base: &str, head: &str) -> Result<Vec<Value>, String> {
    if !crate::pull_request::is_oid(base) || !crate::pull_request::is_oid(head) {
        return Err(crate::i18n::text("backend-invalid-bitbucket-diffstat-revision-e74ea5").into());
    }
    pages(
        api,
        &format!(
            "{}/diffstat/{head}..{base}?topic=false&renames=true&pagelen=100",
            endpoint(&api.hosting)?
        ),
    )
}

fn parse(v: &Value, h: &Hosting) -> Result<PullRequest, String> {
    let n = v
        .get("id")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or(crate::i18n::text(
            "backend-bitbucket-pr-has-no-native-id-f795d6",
        ))?;
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
        .map(|id| json!({"uuid":id}))
        .collect::<Vec<_>>();
    if n.reviewers
        .iter()
        .any(|s| s.is_empty() || s.chars().any(char::is_control))
    {
        return Err(
            crate::i18n::text("backend-bitbucket-reviewers-must-be-native-uuids-7d5dc3").into(),
        );
    }
    let v=api.send("POST",&format!("{}/pullrequests",endpoint(&api.hosting)?),Some(&json!({"title":n.title,"description":n.body,"draft":n.draft,"source":{"branch":{"name":n.branch},"repository":{"uuid":repo["uuid"]}},"destination":{"branch":{"name":n.base}},"reviewers":reviewers})))?;
    parse(&v, &api.hosting).map(|p| p.url).map_err(|e| {
        crate::i18n::message(
            "backend-create-response-unreadable-refresh-before-creating-again-e-bb0176",
            &[("detail", (e).to_string().into())],
        )
    })
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
            return Err(crate::i18n::text("backend-invalid-target-branch-d8264c").into());
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
