//! Connection inventory and evidence from Git's real transport, without retaining trace secrets.
mod display;
pub(crate) use display::line as display_line;

use crate::git;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub(crate) struct Inventory {
    pub remotes: Vec<git::RemoteEndpoint>,
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
fn config(repo: &Path, key: &str, url: Option<&str>) -> Result<Option<String>, String> {
    let raw = match url {
        Some(url) => git::connection_read(repo, &["config", "--null", "--get-urlmatch", key, url]),
        None => git::connection_read(repo, &["config", "--null", "--get", key]),
    }?;
    if raw.is_empty() {
        Ok(None)
    } else {
        raw.strip_suffix('\0')
            .map(|v| Some(v.into()))
            .ok_or_else(|| "Invalid network configuration response".into())
    }
}
fn disabled(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "false" | "no" | "off" | "0"
        )
    })
}
const TLS_ENV: &str =
    "case \"$GIT_SSL_NO_VERIFY\" in ''|false|FALSE|no|NO|off|OFF|0) ;; *) printf disabled;; esac";
pub(crate) fn inventory(repo: &Path, chosen: Option<&str>) -> Result<Inventory, String> {
    let remotes = git::remote_endpoints(repo)?;
    let selected = chosen
        .and_then(|name| remotes.iter().find(|e| e.key() == name))
        .or_else(|| remotes.first());
    let mut lines = Vec::new();
    if let Some(selected) = selected {
        let name = &selected.remote;
        let url = &selected.url;
        lines.push(format!("{}: {}", selected.key(), endpoint(url)));
        let proxy = config(repo, "http.proxy", Some(url))?;
        let override_proxy = config(repo, &format!("remote.{name}.proxy"), None)?;
        lines.push(format!(
            "Git http.proxy: {}",
            match proxy.as_deref() {
                None => "not configured; environment applies".into(),
                Some("") => "explicitly disabled".into(),
                Some(v) => endpoint(v),
            }
        ));
        if let Some(value) = override_proxy {
            lines.push(format!(
                "Git remote proxy override: {}",
                if value.is_empty() {
                    "explicitly disabled".into()
                } else {
                    endpoint(&value)
                }
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
        if git::connection_shell(repo, TLS_ENV)?.trim() == "disabled" {
            lines.push("Git GIT_SSL_NO_VERIFY: certificate verification disabled".into());
        }
        let overrides = git::connection_shell(
            repo,
            "test -z \"$GIT_SSH_COMMAND$GIT_SSH\" || printf configured",
        )?;
        if !overrides.is_empty() {
            lines.push("Git SSH environment override: configured (command withheld; may override SSH config)".into());
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
            "http.proxySSLVerify",
        ] {
            let value = config(repo, key, Some(url))?.unwrap_or_default();
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
        let custom = config(repo, "core.sshCommand", None)?.unwrap_or_default();
        lines.push(format!(
            "SSH command override: {}",
            if custom.is_empty() {
                "none"
            } else {
                "configured (command withheld; may override SSH config)"
            }
        ));
        if let Some(host) = ssh_host(url) {
            let port = url
                .strip_prefix("ssh://")
                .and_then(|s| s.split('/').next())
                .and_then(|a| a.rsplit_once(':'))
                .and_then(|(_, p)| p.parse::<u16>().ok());
            let port = port.map(|p| format!("-p {p} ")).unwrap_or_default();
            let script = format!("ssh -G {port}-- {}", crate::remote::sh_quote(&host));
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
    let userhost = authority.rsplit_once(':').map_or(authority, |(h, p)| {
        if p.parse::<u16>().is_ok() {
            h
        } else {
            authority
        }
    });
    if userhost.chars().filter(|c| *c == '@').count() > 1 {
        return None;
    }
    (!userhost.is_empty()
        && !userhost.starts_with('-')
        && userhost
            .split('@')
            .all(|part| !part.is_empty() && !part.starts_with('-'))
        && userhost
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'@')))
    .then(|| userhost.into())
}
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub lines: Vec<String>,
    pub success: bool,
}
pub(crate) fn test(repo: &Path, name: &str) -> Result<Report, String> {
    let selected = git::remote_endpoints(repo)?
        .into_iter()
        .find(|e| e.key() == name)
        .ok_or("The selected endpoint no longer exists")?;
    if selected.push {
        return Err("Connection tests read the fetch route. Push sign-ins can be inspected and rejected on the HTTPS sign-ins page; write permissions are checked by the actual push.".into());
    }
    let url = selected.url;
    if !(url.starts_with("https://") || url.starts_with("http://") || ssh_host(&url).is_some()) {
        return Err("Choose an HTTPS, HTTP or SSH remote to test a network connection".into());
    }
    let unverified = url.starts_with("https://")
        && (disabled(config(repo, "http.sslVerify", Some(&url))?.as_deref())
            || disabled(config(repo, "http.proxySSLVerify", Some(&url))?.as_deref())
            || git::connection_shell(repo, TLS_ENV)?.trim() == "disabled");
    let result = git::connection_probe(repo, &selected.remote)?;
    let mut report = classify(
        result.success,
        &result.trace,
        url.starts_with("https://"),
        result.elapsed,
    );
    if unverified {
        report.lines[2]="TLS chain: NOT verified — certificate verification is disabled in Git's configuration or environment".into();
    }
    if url.starts_with("http://") {
        report.lines[2] = "TLS chain: not used for an unencrypted HTTP remote".into();
    }
    Ok(report)
}
fn classify(success: bool, trace: &str, tls: bool, elapsed: std::time::Duration) -> Report {
    let mut failure = None;
    let mut completed = [false; 4];
    // curl writes successful certificate details too. Read explicit events in order;
    // an earlier successful TLS exchange must not mask a later auth/connection error.
    for text in trace.lines() {
        let t = text.to_ascii_lowercase();
        if t.contains("was resolved") {
            completed[0] = true;
        }
        if t.contains("connected to ") {
            completed[0] = true;
            completed[1] = true;
        }
        if t.contains("ssl certificate verify ok")
            || t.contains("ssl connection using")
            || t.contains("ssl certificate verification ok")
        {
            completed[0] = true;
            completed[1] = true;
            completed[2] = true;
        }
        let event = if t.contains("407")
            && (t.contains("http") || t.contains("proxy") || t.contains("connect"))
        {
            Some((1, "407: the proxy wants a password"))
        } else if t.contains("could not resolve") || t.contains("name or service not known") {
            Some((0, "DNS: the host or proxy could not be resolved"))
        } else if t.contains("host key verification failed") {
            Some((
                2,
                "SSH host verification failed; verify the server's host key on the repository machine",
            ))
        } else if [
            "ssl certificate problem",
            "certificate verify failed",
            "certificate verification failed",
            "server certificate verification failed",
            "ssl connect error",
            "ssl_error_",
            "tls handshake failed",
            "tls connect error",
            "ssl peer certificate",
        ]
        .iter()
        .any(|pattern| t.contains(pattern))
        {
            Some((
                2,
                "TLS: certificate chain or handshake failed; check the configured CA and hostname",
            ))
        } else if git::is_auth_error(text)
            || t.contains("publickey")
            || t.contains("access denied")
            || [
                "http/2 401",
                "http/2 403",
                "http/1.1 401",
                "http/1.1 403",
                "http/1.0 401",
                "http/1.0 403",
            ]
            .iter()
            .any(|status| t.contains(status))
        {
            Some((
                3,
                "Authentication: the server refused this account or no unattended sign-in is available",
            ))
        } else if [
            "failed to connect",
            "connection refused",
            "timed out",
            "no route",
            "connection reset",
            "recv failure",
            "send failure",
            "empty reply from server",
        ]
        .iter()
        .any(|pattern| t.contains(pattern))
        {
            Some((
                1,
                "TCP: could not connect to the server or proxy, or the connection failed",
            ))
        } else {
            None
        };
        if event.is_some() {
            failure = event;
        }
    }
    let mut lines = Vec::new();
    let names = [
        "DNS",
        "TCP",
        if tls {
            "TLS chain (Git's configured CA trust)"
        } else {
            "SSH transport handshake"
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
        } else if completed[i] && failure.is_none_or(|(step, _)| i != step) {
            "completed earlier in this trace"
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
    fn actual_git_inventory_distinguishes_empty_proxy_and_redacts_configured_secrets()
    -> Result<(), String> {
        let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _owned = Cleanup(repo.parent().ok_or("No fixture root")?.into());
        git::practice_run(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://example.invalid/team/repo",
            ],
        )?;
        git::practice_run(
            &repo,
            &[
                "config",
                "http.proxy",
                "http://user:NEVER_SHOW_SECRET@proxy.invalid:8443/private",
            ],
        )?;
        git::practice_run(
            &repo,
            &["config", "http.sslCAInfo", "/private/NEVER_SHOW_CA_PATH"],
        )?;
        let result = inventory(&repo, Some("origin"))?;
        let text = result.lines.join("\n");
        assert!(text.contains("http://proxy.invalid:8443"));
        assert!(text.contains("custom CA location configured"));
        assert!(!text.contains("NEVER_SHOW_SECRET"));
        assert!(!text.contains("NEVER_SHOW_CA_PATH"));
        git::practice_run(&repo, &["config", "http.proxy", ""])?;
        assert_eq!(
            config(
                &repo,
                "http.proxy",
                Some("https://example.invalid/team/repo")
            )?,
            Some(String::new())
        );
        assert!(
            inventory(&repo, Some("origin"))?
                .lines
                .iter()
                .any(|line| line == "Git http.proxy: explicitly disabled")
        );
        Ok(())
    }
    #[test]
    fn disabled_certificate_verification_cannot_be_reported_as_verified() {
        for value in ["false", "0", "off", "no"] {
            assert!(disabled(Some(value)));
        }
        assert!(!disabled(None));
        assert!(!disabled(Some("true")));
    }
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
            Some("git@example.org".into())
        );
        assert_eq!(ssh_host("ssh://-oProxyCommand=bad/r"), None);
        assert_eq!(ssh_host("file:///tmp/r"), None);
    }
}

#[cfg(test)]
mod trace_regressions {
    use super::*;
    #[test]
    fn successful_tls_followed_by_auth_failure_is_not_a_certificate_failure() {
        for code in [401, 403] {
            let report = classify(
                false,
                &format!(
                    "Connected to fixture.invalid\nServer certificate: CN fixture.invalid\nSSL certificate verify ok\n<= Recv header: HTTP/2 {code}\nfatal: the requested URL returned error: {code}"
                ),
                true,
                std::time::Duration::ZERO,
            );
            assert!(report.lines[2].contains("completed"), "{:?}", report.lines);
            assert!(report.lines[3].ends_with("failed"), "{:?}", report.lines);
            assert!(
                report
                    .lines
                    .iter()
                    .any(|line| line.starts_with("Authentication: the server"))
            );
        }
    }
    #[test]
    fn later_connection_failure_preserves_earlier_tls_evidence() {
        let report = classify(
            false,
            "SSL certificate verify ok\nRecv failure: Connection reset by peer",
            true,
            std::time::Duration::ZERO,
        );
        assert!(report.lines[1].ends_with("failed"));
        assert!(report.lines[2].contains("completed earlier"));
        let report = classify(
            false,
            "Connected to fixture.invalid\nSSL certificate problem: self-signed certificate",
            true,
            std::time::Duration::ZERO,
        );
        assert!(report.lines[2].ends_with("failed"));
        assert!(!report.lines[3].ends_with("failed"));
    }
}
