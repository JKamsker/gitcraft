//! Deliberately small transport allowlist. Userinfo/query/fragment never survive export;
//! remote helpers, command transports and ambiguous authority syntax are not portable data.
use super::{host, text};
pub(crate) fn sanitize_url(input: &str) -> Result<(String, bool), String> {
    text(input, 4096)?;
    if let Some(rest) = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("ssh://"))
    {
        let ssh = input.starts_with("ssh://");
        let scheme = if ssh { "ssh://" } else { "https://" };
        let (authority, path) = rest
            .split_once('/')
            .ok_or("A portable URL needs a repository path")?;
        let (userinfo, server) = authority
            .rsplit_once('@')
            .map_or((None, authority), |(u, h)| (Some(u), h));
        host(server)?;
        let mut host_parts = server.split(':');
        let _ = host_parts.next();
        if let Some(port) = host_parts.next() {
            if host_parts.next().is_some() || port.parse::<u16>().ok().is_none_or(|p| p == 0) {
                return Err("Invalid remote port".into());
            }
        }
        let path = path.split(['?', '#']).next().unwrap_or_default();
        if path.is_empty()
            || path.contains('\\')
            || path.starts_with('-')
            || path.split('/').any(|p| p == ".." || p == ".")
        {
            return Err("Invalid repository URL path".into());
        }
        let user = if ssh {
            userinfo
                .map(|u| u.split(':').next().unwrap_or_default())
                .filter(|u| !u.is_empty())
                .map(|u| {
                    if !u
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                        || u.starts_with('-')
                    {
                        Err("Invalid SSH transport username")
                    } else {
                        Ok(format!("{u}@"))
                    }
                })
                .transpose()?
                .unwrap_or_default()
        } else {
            String::new()
        };
        let safe = format!("{scheme}{user}{server}/{path}");
        return Ok((safe.clone(), safe != input));
    }
    if !input.contains("://") && !input.starts_with(['-', '/', '\\']) {
        if let Some((server, path)) = input.split_once(':') {
            let (user, server) = server
                .split_once('@')
                .ok_or("Use an explicit SSH username in scp-style URLs")?;
            host(server)?;
            if user.is_empty()
                || user.starts_with('-')
                || !user
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                || path.is_empty()
                || path.starts_with(['-', '/', '\\'])
                || path.contains(['?', '#', '\\'])
                || path.split('/').any(|p| p == "." || p == "..")
            {
                return Err("Invalid scp-style repository URL".into());
            }
            return Ok((input.into(), false));
        }
    }
    Err("Only credential-free HTTPS and SSH remote URLs can be imported; local sources must be selected separately on this machine".into())
}
