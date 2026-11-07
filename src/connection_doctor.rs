//! Connection inventory and evidence from Git's real transport, without retaining trace secrets.
use crate::git;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub(crate) struct Inventory {
    pub remotes: Vec<(String, String)>,
    pub lines: Vec<String>,
}
/// A display endpoint never includes a user, password, path, query, or command text.
pub(crate) fn endpoint(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "none".into();
    }
    let (scheme, rest) = value.split_once("://").unwrap_or(("proxy", value));
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    if authority.is_empty()
        || authority
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-'))
    {
        "configured (value withheld)".into()
    } else {
        format!("{scheme}://{authority}")
    }
}
fn config(repo: &Path, key: &str, url: Option<&str>) -> Result<String, String> {
    match url {
        Some(url) => git::connection_read(repo, &["config", "--get-urlmatch", key, url]),
        None => git::connection_read(repo, &["config", "--get", key]),
    }
}
pub(crate) fn inventory(repo: &Path, chosen: Option<&str>) -> Result<Inventory, String> {
    let remotes: Vec<_> = git::remotes(repo)?
        .into_iter()
        .map(|r| (r.name, r.url))
        .collect();
    let selected = chosen
        .and_then(|name| remotes.iter().find(|(n, _)| n == name))
        .or_else(|| remotes.first());
    let mut lines = Vec::new();
    if let Some((name, url)) = selected {
        lines.push(format!("Remote {name}: {}", endpoint(url)));
        let proxy = config(repo, "http.proxy", Some(url))?;
        let override_proxy = config(repo, &format!("remote.{name}.proxy"), None)?;
        lines.push(format!("Git http.proxy: {}", endpoint(&proxy)));
        if !override_proxy.is_empty() {
            lines.push(format!(
                "Git remote proxy override: {}",
                endpoint(&override_proxy)
            ));
        }
        // Environment on the repository's machine, never copied from this computer to SSH/WSL.
        let env = git::connection_read(
            repo,
            &[
                "-c",
                "alias.gitcraft-network-env=!printf '%s\\n' \"https_proxy=$https_proxy\" \"HTTPS_PROXY=$HTTPS_PROXY\" \"http_proxy=$http_proxy\" \"HTTP_PROXY=$HTTP_PROXY\" \"all_proxy=$all_proxy\" \"ALL_PROXY=$ALL_PROXY\" \"no_proxy=$no_proxy\" \"NO_PROXY=$NO_PROXY\"",
                "gitcraft-network-env",
            ],
        )?;
        for line in env.lines() {
            if let Some((key, value)) = line.split_once('=')
                && !value.is_empty()
            {
                lines.push(if key.eq_ignore_ascii_case("no_proxy") {
                    format!("Git {key}: configured bypass list")
                } else {
                    format!("Git {key}: {}", endpoint(value))
                });
            }
        }
        lines.push(format!(
            "Forge REST client (this computer): {}",
            crate::forge::environment_proxy(url)
                .map(|p| endpoint(&p))
                .unwrap_or_else(|| "direct (including NO_PROXY bypass)".into())
        ));
        lines.push(
            "GitHub / GitLab CLI clients use their own CLI configuration and environment.".into(),
        );
        for key in [
            "http.sslCAInfo",
            "http.sslCAPath",
            "http.sslBackend",
            "http.sslVerify",
        ] {
            let value = config(repo, key, Some(url))?;
            lines.push(format!(
                "{key}: {}",
                if value.is_empty() {
                    "Git default".into()
                } else if key.ends_with("Info") || key.ends_with("Path") {
                    "custom CA location configured".into()
                } else {
                    crate::diagnostics::crash::scrub(value.trim())
                }
            ));
        }
        let custom = config(repo, "core.sshCommand", None)?;
        lines.push(format!(
            "SSH command override: {}",
            if custom.is_empty() {
                "none"
            } else {
                "configured (command withheld; may override SSH config)"
            }
        ));
        if let Some(host) = ssh_host(url) {
            let script = format!("ssh -G -- {}", crate::remote::sh_quote(&host));
            // The managed Git alias supplies the repository machine and bounded I/O.
            match git::connection_shell(repo, &script) {
                Ok(text) => for key in ["proxyjump", "proxycommand"] {
                    let val = text.lines().find_map(|l|l.strip_prefix(key).and_then(|s|s.strip_prefix(' '))).unwrap_or("none");
                    lines.push(format!("SSH {key}: {}",if val=="none" {"none"}else{"configured (value withheld)"}));
                },
                Err(_) => lines.push("SSH ProxyJump / ProxyCommand: ssh -G unavailable; inspect SSH config on the repository machine".into()),
            }
        } else {
            lines.push("SSH ProxyJump / ProxyCommand: applies to SSH remotes".into());
        }
    }
    Ok(Inventory { remotes, lines })
}
fn ssh_host(url: &str) -> Option<String> {
    let authority = if let Some(rest) = url.strip_prefix("ssh://") {
        rest.split('/').next()?
    } else if !url.contains("://") {
        url.split_once(':')?.0
    } else {
        return None;
    };
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')))
    .then(|| host.into())
}
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub lines: Vec<String>,
    pub success: bool,
}
pub(crate) fn test(repo: &Path, name: &str) -> Result<Report, String> {
    let url = git::remotes(repo)?
        .into_iter()
        .find(|r| r.name == name)
        .ok_or("The selected remote no longer exists")?
        .url;
    if !(url.starts_with("https://") || url.starts_with("http://") || ssh_host(&url).is_some()) {
        return Err("Choose an HTTPS, HTTP or SSH remote to test a network connection".into());
    }
    let result = git::connection_probe(repo, name)?;
    let mut report = classify(
        result.success,
        &result.trace,
        url.starts_with("https://"),
        result.elapsed,
    );
    if url.starts_with("http://") {
        report.lines[2] = "TLS chain: not used for an unencrypted HTTP remote".into();
    }
    Ok(report)
}
fn classify(success: bool, trace: &str, tls: bool, elapsed: std::time::Duration) -> Report {
    let t = trace.to_lowercase();
    let failure = if t.contains("407") {
        Some((1, "407: the proxy wants a password"))
    } else if t.contains("could not resolve")
        || t.contains("name or service not known")
        || t.contains("could not resolve hostname")
    {
        Some((0, "DNS: the host or proxy could not be resolved"))
    } else if t.contains("host key verification failed") {
        Some((
            2,
            "SSH host verification failed; verify the server's host key on the repository machine",
        ))
    } else if t.contains("certificate")
        || t.contains("ssl connect")
        || t.contains("tls") && t.contains("failed")
    {
        Some((
            2,
            "TLS: certificate chain or handshake failed; check the configured CA and hostname",
        ))
    } else if git::is_auth_error(trace) || t.contains("publickey") || t.contains("access denied") {
        Some((
            3,
            "Authentication: the server refused this account or no unattended sign-in is available",
        ))
    } else if t.contains("failed to connect")
        || t.contains("connection refused")
        || t.contains("timed out")
        || t.contains("no route")
    {
        Some((1, "TCP: could not connect to the server or proxy"))
    } else {
        None
    };
    let mut lines = Vec::new();
    let names = [
        "DNS",
        "TCP",
        if tls {
            "TLS chain (Git's configured CA trust)"
        } else {
            "SSH host-key / transport verification"
        },
        "Authentication",
    ];
    for (i, name) in names.into_iter().enumerate() {
        let state = if success {
            if i == 3 {
                "accepted for this read (the repository may be public)"
            } else {
                "completed through the configured transport"
            }
        } else if failure.is_some_and(|(step, _)| i < step) {
            "completed before the failure"
        } else if failure.is_some_and(|(step, _)| i == step) {
            "failed"
        } else {
            "not confirmed"
        };
        lines.push(format!("{name}: {state}"));
    }
    if !success {
        lines.push(failure.map(|(_,m)|m.to_owned()).unwrap_or_else(||"Git could not complete ls-remote; inspect the repository operation log for details. No transport stage can be safely inferred.".into()));
    }
    lines.push(format!(
        "ls-remote: {} in {:.2} s",
        if success { "completed" } else { "failed" },
        elapsed.as_secs_f64()
    ));
    Report { lines, success }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_removes_every_secret_component() {
        assert_eq!(
            endpoint("https://user:secret@proxy.example:8443/private?token=x"),
            "https://proxy.example:8443"
        );
        assert_eq!(endpoint("!echo a-secret"), "configured (value withheld)");
    }
    #[test]
    fn stages_name_proxy_and_ca_failures() {
        let proxy = classify(
            false,
            "CONNECT failed, response 407",
            true,
            std::time::Duration::from_secs(1),
        );
        assert!(
            proxy
                .lines
                .iter()
                .any(|s| s.contains("proxy wants a password"))
        );
        let ca = classify(
            false,
            "SSL certificate problem",
            true,
            std::time::Duration::ZERO,
        );
        assert!(ca.lines[2].ends_with("failed"));
        assert!(ca.lines[3].ends_with("not confirmed"));
    }
    #[test]
    fn success_and_unknown_do_not_invent_identity() {
        let r = classify(
            true,
            "Authorization: secret",
            true,
            std::time::Duration::ZERO,
        );
        assert!(r.success);
        assert!(r.lines[3].contains("may be public"));
        assert!(!format!("{r:?}").contains("secret"));
        let r = classify(false, "unknown", true, std::time::Duration::ZERO);
        assert!(r.lines[0].ends_with("not confirmed"));
    }
    #[test]
    fn ssh_destinations_are_not_shell_programs() {
        assert_eq!(
            ssh_host("git@example.org:r.git"),
            Some("example.org".into())
        );
        assert_eq!(ssh_host("ssh://-oProxyCommand=bad/r"), None);
        assert_eq!(ssh_host("file:///tmp/r"), None);
    }
}
