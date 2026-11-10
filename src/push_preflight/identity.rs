//! Endpoint identity and secret-safe labels.
use super::*;

pub(crate) fn target_label(url: &str) -> String {
    location(url)
        .map(|(h, p)| {
            if secret_lines(&p).next().is_some() {
                format!("{h}/[sensitive path]")
            } else {
                format!("{h}/{p}")
            }
        })
        .unwrap_or_else(|| {
            let path = if let Some(rest) = url.strip_prefix("file://") {
                let rest = rest.split(['?', '#']).next().unwrap_or(rest);
                if let Some((authority, path)) = rest.split_once('/') {
                    format!(
                        "{}/{}",
                        authority.rsplit('@').next().unwrap_or(authority),
                        path
                    )
                } else {
                    rest.rsplit('@').next().unwrap_or(rest).to_owned()
                }
            } else {
                url.to_owned()
            };
            format!("Local repository: {}", display_path(&path))
        })
}

pub(crate) fn display_path(path: &str) -> String {
    if secret_lines(path).next().is_some() {
        "[sensitive path]".into()
    } else {
        path.chars()
            .flat_map(|c| {
                if c.is_control() {
                    c.escape_default().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect()
    }
}

pub(crate) fn location(url: &str) -> Option<(String, String)> {
    let url = url.split(['?', '#']).next()?.trim();
    let (host, path, default_port) = if let Some((scheme, rest)) = url.split_once("://") {
        if !["https", "http", "ssh", "git"].contains(&scheme) {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let default_port = match scheme {
            "https" => "443",
            "http" => "80",
            "ssh" => "22",
            "git" => "9418",
            _ => return None,
        };
        (authority.rsplit('@').next()?, path, Some(default_port))
    } else {
        if url.contains('\\') || url.starts_with('/') || url.starts_with('.') {
            return None;
        }
        let (authority, path) = url.split_once(':')?;
        (authority.rsplit('@').next()?, path, None)
    };
    if host.len() < 2 || path.is_empty() || host.chars().any(char::is_control) {
        return None;
    }
    let lower = host.to_ascii_lowercase();
    let host = default_port
        .and_then(|port| lower.strip_suffix(&format!(":{port}")))
        .unwrap_or(&lower);
    let host = match host {
        "ssh.github.com" | "ssh.github.com:443" => "github.com".to_owned(),
        "altssh.gitlab.com" | "altssh.gitlab.com:443" => "gitlab.com".to_owned(),
        host => host.to_owned(),
    };
    Some((host, path.trim_matches('/').trim_end_matches(".git").into()))
}
