//! Repository-independent gh API read retaining status and headers even on HTTP errors.
use crate::forge::discovery::{BODY_LIMIT, DiscoveryError, Response};

pub(in crate::forge) fn get(
    account: &crate::connections::Account,
    endpoint: &str,
) -> Result<Response, DiscoveryError> {
    let mut command = crate::connections::discovery_command(
        account,
        |command| super::super::github::discovery::verify_identity(run(command)?, &account.user),
        |_| DiscoveryError::AuthRefused,
    )?;
    command.args([
        "api",
        "--hostname",
        "github.com",
        "--include",
        "--method",
        "GET",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
        endpoint,
    ]);
    run(command)
}
fn run(command: std::process::Command) -> Result<Response, DiscoveryError> {
    let output = crate::processes::run(
        command,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: BODY_LIMIT + 32768,
            stderr_limit: Some(8192),
            ..Default::default()
        },
    )
    .and_then(|output| output.complete())
    .map_err(|_| DiscoveryError::Transport)?;
    parse(&output.stdout)
}
fn parse(bytes: &[u8]) -> Result<Response, DiscoveryError> {
    let boundary = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .map(|at| (at, 4))
        .or_else(|| {
            bytes
                .windows(2)
                .position(|part| part == b"\n\n")
                .map(|at| (at, 2))
        })
        .ok_or(DiscoveryError::Malformed)?;
    if boundary.0 > 32768 || bytes.len() - boundary.0 - boundary.1 > BODY_LIMIT {
        return Err(DiscoveryError::Malformed);
    }
    let head = std::str::from_utf8(&bytes[..boundary.0]).map_err(|_| DiscoveryError::Malformed)?;
    let mut lines = head.lines();
    let first = lines.next().ok_or(DiscoveryError::Malformed)?;
    if !first.starts_with("HTTP/") {
        return Err(DiscoveryError::Malformed);
    }
    let status = first
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .ok_or(DiscoveryError::Malformed)?;
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_owned(), value.trim().to_owned()))
        .collect();
    Ok(Response {
        status,
        headers,
        body: bytes[boundary.0 + boundary.1..].to_vec(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_retains_error_headers() -> Result<(), DiscoveryError> {
        let response = parse(b"HTTP/2.0 403 Forbidden\r\nX-RateLimit-Remaining: 0\r\n\r\n{}")?;
        assert_eq!(response.status, 403);
        assert_eq!(response.header("x-ratelimit-remaining"), Some("0"));
        assert!(parse(b"not HTTP").is_err());
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[test]
fn discovery_user_verification_preserves_nonzero_rate_limit_headers()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("gh");
    std::fs::write(
        &path,
        "#!/bin/sh\ncase \"$1 $2\" in\n'auth token') printf '%s' fixture-token;;\n'api user') printf 'HTTP/2.0 403 Forbidden\\r\\nX-RateLimit-Remaining: 0\\r\\nX-RateLimit-Reset: 2000000000\\r\\nRetry-After: 60\\r\\n\\r\\n{}'; exit 1;;\n*) exit 9;;\nesac\n",
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    struct Restore(Option<std::path::PathBuf>);
    impl Drop for Restore {
        fn drop(&mut self) {
            crate::connections::TEST_PROGRAMS.with(|programs| {
                let mut programs = programs.borrow_mut();
                if let Some(previous) = self.0.take() {
                    programs.insert("gh".into(), previous);
                } else {
                    programs.remove("gh");
                }
            });
        }
    }
    let _restore = Restore(
        crate::connections::TEST_PROGRAMS
            .with(|programs| programs.borrow_mut().insert("gh".into(), path)),
    );
    let account = crate::connections::Account {
        host: "github.com".into(),
        user: "selected-user".into(),
        config_dir: String::new(),
    };
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let result = get(&account, "/user/repos");
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    assert!(
        matches!(result, Err(DiscoveryError::RateLimited { retry_at: Some(time) }) if (before + 60..=after + 60).contains(&time))
    );
    let raw = parse(b"HTTP/2.0 403 Forbidden\r\nX-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 2000000000\r\n\r\n{}");
    assert!(matches!(
        raw.and_then(|raw| super::super::github::discovery::verify_identity(raw, "selected-user")),
        Err(DiscoveryError::RateLimited {
            retry_at: Some(2000000000)
        })
    ));
    Ok(())
}
