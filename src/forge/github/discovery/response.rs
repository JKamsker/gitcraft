use crate::forge::discovery::*;
use crate::forge::{Forge, Hosting};
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiOwner {
    id: u64,
    login: String,
    #[serde(rename = "type")]
    kind: String,
}
#[derive(Deserialize)]
struct ApiRepository {
    id: u64,
    name: String,
    owner: ApiOwner,
    private: bool,
    visibility: Option<String>,
    description: Option<String>,
    language: Option<String>,
    stargazers_count: Option<u64>,
}
fn repository(value: serde_json::Value, public: bool) -> Result<Repository, DiscoveryError> {
    let raw: ApiRepository =
        serde_json::from_value(value).map_err(|_| DiscoveryError::Malformed)?;
    let visibility = match raw.visibility.as_deref() {
        Some("public") if !raw.private => Visibility::Public,
        Some("private") if raw.private => Visibility::Private,
        Some("internal") if raw.private => Visibility::Internal,
        None => {
            if raw.private {
                Visibility::Private
            } else {
                Visibility::Public
            }
        }
        _ => return Err(DiscoveryError::Malformed),
    };
    if public && visibility != Visibility::Public {
        return Err(DiscoveryError::Malformed);
    }
    let plain = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    if !plain(&raw.owner.login) || !plain(&raw.name) {
        return Err(DiscoveryError::Malformed);
    }
    let path = format!("{}/{}", raw.owner.login, raw.name);
    let web_url = format!("https://github.com/{path}");
    Ok(Repository {
        id: raw.id,
        hosting: Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            path: path.clone(),
            web: "https://github.com".into(),
        },
        owner: raw.owner.login,
        name: raw.name,
        description: raw.description.unwrap_or_default(),
        language: raw.language,
        stars: raw.stargazers_count,
        visibility,
        https_url: format!("{web_url}.git"),
        ssh_url: format!("git@github.com:{path}.git"),
        web_url,
    })
}
pub(super) fn page(raw: Response, query: &Query, endpoint: &str) -> Result<Page, DiscoveryError> {
    let number = |name| raw.header(name).and_then(|v| v.parse::<u64>().ok());
    let rate = RateLimit {
        remaining: number("x-ratelimit-remaining"),
        reset: number("x-ratelimit-reset"),
        retry_after: number("retry-after"),
    };
    match raw.status {
        200 => {}
        401 => return Err(DiscoveryError::AuthRefused),
        403 | 429
            if raw.status == 429 || rate.remaining == Some(0) || rate.retry_after.is_some() =>
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            return Err(DiscoveryError::RateLimited {
                retry_at: rate
                    .retry_after
                    .map(|delay| now.saturating_add(delay))
                    .or(rate.reset),
            });
        }
        403 => return Err(DiscoveryError::Forbidden),
        404 => return Err(DiscoveryError::NotFound),
        _ => return Err(DiscoveryError::Transport),
    }
    if raw.body.len() > BODY_LIMIT {
        return Err(DiscoveryError::Malformed);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&raw.body).map_err(|_| DiscoveryError::Malformed)?;
    let searching = matches!(query, Query::SearchOwners(_) | Query::SearchRepositories(_));
    let items = if searching { &value["items"] } else { &value }
        .as_array()
        .ok_or(DiscoveryError::Malformed)?;
    if items.len() > 50 {
        return Err(DiscoveryError::Malformed);
    }
    let total = value.get("total_count").and_then(|v| v.as_u64());
    let incomplete = value
        .get("incomplete_results")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let next = raw.header("link").and_then(|link| {
        link.split(',').find_map(|part| {
            let (target, rel) = part.trim().split_once(';')?;
            if !rel.trim().eq("rel=\"next\"") {
                return None;
            }
            super::endpoints::validate_next(
                endpoint,
                target.trim().strip_prefix('<')?.strip_suffix('>')?,
            )
        })
    });
    let mut repositories = Vec::new();
    let mut owners = Vec::new();
    for item in items {
        if matches!(query, Query::SearchOwners(_)) {
            let owner: ApiOwner =
                serde_json::from_value(item.clone()).map_err(|_| DiscoveryError::Malformed)?;
            if !matches!(owner.kind.as_str(), "User" | "Organization") {
                continue;
            }
            owners.push(Owner {
                id: owner.id,
                login: owner.login,
                organization: owner.kind == "Organization",
            });
        } else {
            repositories.push(repository(item.clone(), query.public())?);
        }
    }
    Ok(Page {
        repositories,
        owners,
        next,
        total,
        incomplete,
        rate,
    })
}
