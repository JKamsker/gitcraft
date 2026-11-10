//! Environment proxy selection and NO_PROXY matching.

/// `builder` going through the proxy the environment sets for `web`, as git and curl pick it.
pub(crate) fn with_proxy(builder: ureq::AgentBuilder, web: &str) -> ureq::AgentBuilder {
    let proxy = proxy_for(web, |name| {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    });
    match proxy.and_then(|url| ureq::Proxy::new(url).ok()) {
        Some(proxy) => builder.proxy(proxy),
        None => builder,
    }
}

/// The exact environment choice used by the REST client. Never display it without redaction.
pub(crate) fn environment_proxy(web: &str) -> Option<String> {
    proxy_for(web, |name| {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    })
}

/// The proxy for `web` (`https://host[:port]`): the scheme's own variable, else `ALL_PROXY`, unless
/// `NO_PROXY` names the host (`*`, a domain and its subdomains, an IPv4 range like `10.0.0.0/8`).
pub(super) fn proxy_for(web: &str, var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let (scheme, rest) = web.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let host = match authority.find(']') {
        Some(end) if authority.starts_with('[') => &authority[1..end],
        _ => authority.split(':').next().unwrap_or(authority),
    }
    .to_lowercase();
    let first = |names: &[&str]| names.iter().find_map(|name| var(name));
    let excluded = first(&["no_proxy", "NO_PROXY"]).unwrap_or_default();
    if excluded
        .split(',')
        .map(|entry| entry.trim().to_lowercase())
        .any(|entry| bypasses(&entry, &host))
    {
        return None;
    }
    match scheme {
        "https" => first(&["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]),
        _ => first(&["http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"]),
    }
}

/// Whether one `NO_PROXY` entry covers `host`.
pub(super) fn bypasses(entry: &str, host: &str) -> bool {
    if entry == "*" {
        return true;
    }
    if let Some((net, bits)) = entry.split_once('/') {
        let (Ok(net), Ok(bits), Ok(ip)) = (
            net.parse::<std::net::Ipv4Addr>(),
            bits.parse::<u32>(),
            host.parse::<std::net::Ipv4Addr>(),
        ) else {
            return false;
        };
        let mask = u32::MAX.checked_shl(32 - bits.min(32)).unwrap_or(0);
        return u32::from(net) & mask == u32::from(ip) & mask;
    }
    let entry = entry.trim_start_matches("*.").trim_start_matches('.');
    let entry = match entry.find(']') {
        Some(end) if entry.starts_with('[') => &entry[1..end],
        _ if entry.matches(':').count() == 1 => entry.split(':').next().unwrap_or(entry),
        _ => entry,
    };
    !entry.is_empty() && (host == entry || host.ends_with(&format!(".{entry}")))
}
