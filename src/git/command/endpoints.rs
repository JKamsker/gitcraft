//! Effective endpoint metadata. Expanded URLs are never fed back into a mutating Git call.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteEndpoint {
    pub remote: String,
    pub push: bool,
    pub index: usize,
    pub url: String,
}
impl RemoteEndpoint {
    pub fn key(&self) -> String {
        format!(
            "{} · {} {}",
            self.remote,
            if self.push { "push" } else { "fetch" },
            self.index + 1
        )
    }
}
pub(crate) fn remote_endpoints(repo: &Path) -> Result<Vec<RemoteEndpoint>, String> {
    let names = connection_read(repo, &["remote"])?;
    let mut endpoints = Vec::new();
    for name in names.lines().filter(|n| !n.is_empty()) {
        for push in [false, true] {
            let args = if push {
                vec!["remote", "get-url", "--push", "--all", name]
            } else {
                vec!["remote", "get-url", "--all", name]
            };
            for (index, url) in connection_read(repo, &args)?.lines().enumerate() {
                if !url.is_empty() {
                    endpoints.push(RemoteEndpoint {
                        remote: name.into(),
                        push,
                        index,
                        url: url.into(),
                    });
                }
            }
        }
    }
    Ok(endpoints)
}
/// The failure belongs to this captured profile and these actual command destinations.
#[derive(Clone)]
pub(crate) struct AuthFailure {
    pub context: crate::connections::Context,
    pub endpoints: Vec<String>,
    pub confirmed: bool,
}
fn safe_http_url(value: &str) -> Option<String> {
    let (scheme, rest) = value.split_once("://")?;
    if !matches!(scheme, "https" | "http") {
        return None;
    }
    let (authority, tail) = rest
        .split_once('/')
        .map_or((rest, String::new()), |(a, p)| (a, format!("/{p}")));
    let authority = authority.rsplit('@').next()?;
    let clean = format!("{scheme}://{authority}{tail}");
    crate::git::CredentialScope::for_url(&clean, "endpoint-check").ok()?;
    Some(clean)
}
impl AuthFailure {
    /// An explicit URL reported by Git/LFS takes precedence over configured candidates.
    /// Otherwise the UI requires a destination choice; it never guesses a fetch URL.
    pub fn from_error(error: &str, attempted: &[String]) -> Self {
        let reported = error
            .split(['\'', '"', ' ', '\n', '\r', '\t'])
            .filter_map(safe_http_url)
            .next();
        let confirmed = reported.is_some();
        let mut endpoints = reported.into_iter().collect::<Vec<_>>();
        if endpoints.is_empty() {
            endpoints.extend(attempted.iter().filter_map(|u| safe_http_url(u)));
        }
        endpoints.dedup();
        Self {
            context: crate::connections::current(),
            endpoints,
            confirmed,
        }
    }
}
/// Inspect the operation's own remote selection, including implicit branch push overrides.
/// Only metadata is expanded; the original command retains Git's normal routing semantics.
pub(crate) fn operation_endpoints(repo: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let args = after_options(args);
    let Some(sub) = args.first() else {
        return Ok(vec![]);
    };
    let push = *sub == "push";
    if !matches!(*sub, "push" | "fetch" | "pull") {
        return Ok(vec![]);
    }
    let all = remote_endpoints(repo)?;
    if args.contains(&"--all") && *sub == "fetch" {
        return Ok(all.into_iter().filter(|e| !e.push).map(|e| e.url).collect());
    }
    let mut explicit = None;
    let mut rest = &args[1..];
    while let Some(word) = rest.first() {
        if *word == "--" {
            explicit = rest.get(1).copied();
            break;
        }
        if let Some(name) = word.strip_prefix("--repo=") {
            explicit = Some(name);
            break;
        }
        if matches!(
            *word,
            "--repo"
                | "--receive-pack"
                | "--exec"
                | "--upload-pack"
                | "-o"
                | "--push-option"
                | "--depth"
                | "--deepen"
                | "--shallow-since"
                | "--shallow-exclude"
                | "--filter"
                | "-j"
                | "--jobs"
        ) {
            rest = rest.get(2..).unwrap_or_default();
            continue;
        }
        if !word.starts_with('-') {
            explicit = Some(word);
            break;
        }
        rest = &rest[1..];
    }
    let remote = if let Some(name) = explicit {
        name.to_owned()
    } else {
        let branch = connection_read(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
        let read = |key: &str| {
            connection_read(repo, &["config", "--get", key]).map(|v| v.trim().to_owned())
        };
        let mut selected = String::new();
        if push {
            selected = read(&format!("branch.{}.pushRemote", branch.trim()))?;
            if selected.is_empty() {
                selected = read("remote.pushDefault")?;
            }
        }
        if selected.is_empty() {
            selected = read(&format!("branch.{}.remote", branch.trim()))?;
        }
        if selected.is_empty() {
            selected = "origin".into();
        }
        selected
    };
    let matched: Vec<_> = all
        .into_iter()
        .filter(|e| e.remote == remote && e.push == push)
        .map(|e| e.url)
        .collect();
    Ok(if matched.is_empty() {
        safe_http_url(&remote).into_iter().collect()
    } else {
        matched
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reported_push_and_lfs_endpoint_override_fetch_guesses_without_userinfo() {
        let candidates = vec![
            "https://push-a.invalid/repo".into(),
            "https://push-b.invalid/repo".into(),
        ];
        let failure = AuthFailure::from_error(
            "fatal: Authentication failed for 'https://user:dummy@push-b.invalid/repo'",
            &candidates,
        );
        assert!(failure.confirmed);
        assert_eq!(failure.endpoints, ["https://push-b.invalid/repo"]);
        let lfs = AuthFailure::from_error(
            "LFS: https://lfs.invalid/repo/info/lfs authentication required",
            &candidates,
        );
        assert_eq!(lfs.endpoints, ["https://lfs.invalid/repo/info/lfs"]);
        let unknown = AuthFailure::from_error("Authentication failed", &candidates);
        assert!(!unknown.confirmed);
        assert_eq!(unknown.endpoints, candidates);
    }
}
