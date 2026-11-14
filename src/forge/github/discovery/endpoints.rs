use crate::forge::discovery::{DiscoveryError, Query};

pub(super) fn endpoint(query: &Query) -> Result<String, DiscoveryError> {
    let path = match query {
        Query::Mine => {
            "/user/repos?affiliation=owner,collaborator,organization_member&sort=updated".into()
        }
        Query::Starred => "/user/starred?sort=updated".into(),
        Query::OwnerRepositories {
            login,
            organization,
        } => {
            if login.is_empty()
                || !login
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(DiscoveryError::Malformed);
            }
            format!(
                "/{}/{login}/repos?type={}&sort=updated",
                if *organization { "orgs" } else { "users" },
                if *organization { "public" } else { "owner" }
            )
        }
        Query::SearchOwners(text) | Query::SearchRepositories(text) => {
            if text.trim().is_empty() || text.len() > 256 {
                return Err(DiscoveryError::Malformed);
            }
            let mut url = url::Url::parse("https://api.github.com/search/repositories")
                .map_err(|_| DiscoveryError::Malformed)?;
            if matches!(query, Query::SearchOwners(_)) {
                url.set_path("/search/users");
            }
            url.query_pairs_mut().append_pair("q", text.trim());
            format!("{}?{}", url.path(), url.query().unwrap_or_default())
        }
    };
    Ok(format!("{path}&per_page=50&page=1"))
}

/// Accept only the same endpoint/query, with a strictly advancing page and a fixed size.
pub(super) fn validate_next(current: &str, candidate: &str) -> Option<String> {
    let base = url::Url::parse("https://api.github.com").ok()?;
    let old = base.join(current).ok()?;
    let new = base.join(candidate).ok()?;
    if new.origin() != base.origin()
        || !new.username().is_empty()
        || new.password().is_some()
        || new.fragment().is_some()
        || new.path() != old.path()
    {
        return None;
    }
    let pairs = |url: &url::Url| {
        url.query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let mut old_pairs = pairs(&old);
    let mut new_pairs = pairs(&new);
    let old_page: u32 = old_pairs.remove("page")?.parse().ok()?;
    let new_page: u32 = new_pairs.remove("page")?.parse().ok()?;
    if new_page != old_page + 1 || new_page > 100 || old_pairs != new_pairs {
        return None;
    }
    Some(format!("{}?{}", new.path(), new.query()?))
}
