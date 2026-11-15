//! GitHub discovery; account lists use gh, public queries always use anonymous HTTP.
mod endpoints;
mod response;
#[cfg(test)]
mod tests;
use crate::forge::discovery::*;

pub(crate) fn repositories(
    session: &DiscoverySession,
    query: &Query,
    next: Option<&str>,
) -> Result<Page, DiscoveryError> {
    if !session.account.host.eq_ignore_ascii_case("github.com")
        && session.account.selected()
        && !query.public()
    {
        return Err(DiscoveryError::Forbidden);
    }
    let first = endpoints::endpoint(query)?;
    let endpoint = if let Some(next) = next {
        // Validate against the original query as well as against page progression at parsing.
        let mut initial = url::Url::parse(&format!("https://api.github.com{first}"))
            .map_err(|_| DiscoveryError::Malformed)?;
        let supplied = url::Url::parse(&format!("https://api.github.com{next}"))
            .map_err(|_| DiscoveryError::Malformed)?;
        let page: u32 = supplied
            .query_pairs()
            .find(|(k, _)| k == "page")
            .and_then(|(_, v)| v.parse().ok())
            .ok_or(DiscoveryError::Malformed)?;
        if page < 2 {
            return Err(DiscoveryError::Malformed);
        }
        let pairs = initial
            .query_pairs()
            .filter(|(k, _)| k != "page")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect::<Vec<_>>();
        initial.set_query(None);
        initial
            .query_pairs_mut()
            .extend_pairs(pairs)
            .append_pair("page", &(page - 1).to_string());
        endpoints::validate_next(
            &format!("{}?{}", initial.path(), initial.query().unwrap_or_default()),
            next,
        )
        .ok_or(DiscoveryError::Malformed)?
    } else {
        first
    };
    let raw = if query.public() {
        public_get(&endpoint)?
    } else {
        if !session.account.selected() {
            return Err(DiscoveryError::AuthRequired);
        }
        crate::forge::cli::discovery::get(&session.account, &endpoint)?
    };
    if !session.valid() || crate::tasks::cancellation().is_cancelled() {
        return Err(DiscoveryError::Cancelled);
    }
    response::page(raw, query, &endpoint)
}
