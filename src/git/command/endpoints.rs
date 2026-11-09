//! Effective endpoint metadata. Expanded URLs are never fed back into a mutating Git call.
use super::*;
mod literal;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteEndpoint {
    pub remote: String,
    pub push: bool,
    pub lfs: bool,
    pub index: usize,
    pub url: String,
}
impl RemoteEndpoint {
    pub fn key(&self) -> String {
        format!(
            "{} · {} {}",
            self.remote,
            if self.lfs {
                "LFS"
            } else if self.push {
                "push"
            } else {
                "fetch"
            },
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
                        lfs: false,
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
    pub binding: Option<RouteBinding>,
    pub lfs: bool,
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
    pub fn needs_separate_account(&self) -> bool {
        if self.lfs {
            return true;
        }
        let Some(binding) = &self.binding else {
            return false;
        };
        if super::after_options(&binding.args.iter().map(String::as_str).collect::<Vec<_>>())
            .first()
            == Some(&"lfs")
        {
            return true;
        }
        // Mixed candidates keep their ordinary bound choices. Foreign entries
        // are separated before offering the dialog, and selection is checked
        // again against the proof at both action entry points.
        !self.endpoints.is_empty()
            && self
                .endpoints
                .iter()
                .all(|url| !binding.authorizes_endpoint(url))
    }
    /// An explicit URL reported by Git/LFS takes precedence over configured candidates.
    /// Otherwise the UI requires a destination choice; it never guesses a fetch URL.
    pub fn from_error(error: &str, attempted: &[String]) -> Self {
        // Only canonical Git auth records and explicit LFS auth-error records count.
        // Untrusted remote banners/help links cannot confirm a destination.
        let mut reported = Vec::new();
        for line in error.lines() {
            let line = line.trim();
            let quoted = line
                .strip_prefix("fatal: Authentication failed for '")
                .and_then(|v| v.strip_suffix('\''))
                .or_else(|| {
                    line.strip_prefix("fatal: unable to access '")
                        .and_then(|v| v.split_once("':"))
                        .filter(|(_, e)| super::is_auth_error(e))
                        .map(|(u, _)| u)
                });
            let lfs = line
                .strip_prefix("LFS: ")
                .filter(|_| {
                    line.to_ascii_lowercase()
                        .contains("authentication required")
                })
                .and_then(|v| v.split_whitespace().next());
            if let Some(url) = quoted.or(lfs).and_then(safe_http_url)
                && !reported.contains(&url)
            {
                reported.push(url);
            }
        }
        let confirmed = reported.len() == 1;
        let mut endpoints = reported;
        if !confirmed {
            for url in attempted.iter().filter_map(|u| safe_http_url(u)) {
                if !endpoints.contains(&url) {
                    endpoints.push(url);
                }
            }
        }
        Self {
            context: crate::connections::current(),
            endpoints,
            confirmed,
            binding: None,
            lfs: error.lines().any(|line| {
                line.trim_start().starts_with("LFS:")
                    || line.contains("git-lfs")
                    || line.contains("Git LFS")
            }),
        }
    }
}
/// Captures every input to Git's route selection, not the fetch-only display model.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct RouteBinding {
    pub args: Vec<String>,
    routes: Vec<String>,
    configuration: String,
}
impl RouteBinding {
    pub fn capture(repo: &Path, args: &[&str]) -> Result<Self, String> {
        let routes = operation_endpoints(repo, args)?;
        let configuration = connection_read(
            repo,
            &[
                "config",
                "--null",
                "--get-regexp",
                r"^(remote\..*\.(url|pushurl|push)|remote\.pushdefault|branch\..*\.(remote|pushremote|merge)|url\..*\.(insteadof|pushinsteadof)|push\.default)$",
            ],
        )?;
        Ok(Self {
            args: args.iter().map(|s| (*s).into()).collect(),
            routes,
            configuration,
        })
    }
    pub fn urls(&self) -> &[String] {
        &self.routes
    }
    /// A reported or explicitly chosen URL cannot enlarge this route proof.
    pub fn authorizes_endpoint(&self, url: &str) -> bool {
        crate::git::CredentialScope::for_url(url, "endpoint-check")
            .is_ok_and(|scope| self.routes.iter().any(|route| scope.matches_url(route)))
    }
    pub fn verify(&self, repo: &Path) -> Result<(), String> {
        let args = self.args.iter().map(String::as_str).collect::<Vec<_>>();
        let current = Self::capture(repo, &args)?;
        self.compare_snapshot(&current)
    }
    fn compare_snapshot(&self, current: &Self) -> Result<(), String> {
        if current == self {
            Ok(())
        } else {
            Err(crate::i18n::text(
                "backend-the-operation-s-effective-fetch-push-destinations-or-routing-conf-943acc",
            )
            .into())
        }
    }
}
/// Inspect the operation's own remote selection, including implicit branch push overrides.
/// Only metadata is expanded; the original command retains Git's normal routing semantics.
pub(crate) fn operation_endpoints(repo: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    let original_args = args;
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
        if *word == "--repo" {
            explicit = rest.get(1).copied();
            break;
        }
        if matches!(
            *word,
            "--receive-pack"
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
    if !matched.is_empty() {
        return Ok(matched);
    }
    // `remote get-url` requires a locally configured remote: a command-only
    // synthetic remote cannot resolve a literal operand. Read only rewrite
    // metadata, retaining the operation's explicit global options, then apply
    // Git's single longest-prefix substitution without starting a transport.
    let query = literal_query(original_args, explicit.is_some())?;
    let configuration = connection_read(repo, &query)?;
    Ok(vec![literal::resolve(&configuration, &remote, push)?])
}

fn literal_query<'a>(original_args: &'a [&'a str], explicit: bool) -> Result<Vec<&'a str>, String> {
    if !explicit {
        return Err(crate::i18n::text("backend-literal-75b6e4c4").into());
    }
    let command = after_options(original_args);
    let mut query = original_args[..original_args.len() - command.len()].to_vec();
    query.extend([
        "config",
        "--null",
        "--get-regexp",
        r"^url\..*\.(insteadof|pushinsteadof)$",
    ]);
    Ok(query)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_metadata_query_keeps_owned_config_without_a_transport() -> Result<(), String> {
        let original = [
            "-c",
            "url.ssh://host/.pushInsteadOf=short:",
            "push",
            "short:repo",
        ];
        assert_eq!(
            literal_query(&original, true)?,
            [
                "-c",
                "url.ssh://host/.pushInsteadOf=short:",
                "config",
                "--null",
                "--get-regexp",
                r"^url\..*\.(insteadof|pushinsteadof)$"
            ]
        );
        assert_eq!(original[2..], ["push", "short:repo"]);
        // A missing implicit match is not a literal URL. Git may select a sole
        // configured remote instead of the conventional origin name.
        for operation in ["fetch", "pull", "push"] {
            assert!(literal_query(&[operation], false).is_err());
        }
        Ok(())
    }
    #[test]
    fn push_only_and_override_configuration_changes_invalidate_route_snapshots() {
        let binding=RouteBinding{args:vec!["push".into(),"origin".into()],routes:vec!["https://old.invalid/repo".into()],configuration:"remote.origin.url\nhttps://fetch.invalid/repo\0remote.origin.pushurl\nhttps://old.invalid/repo\0".into()};
        let mut changed = binding.clone();
        changed.routes = vec!["https://new.invalid/repo".into()];
        assert!(binding.compare_snapshot(&changed).is_err());
        let mut added = binding.clone();
        added.routes.push("https://other.invalid/repo".into());
        assert!(binding.compare_snapshot(&added).is_err());
        for config in [
            "remote.pushdefault\nother\0",
            "branch.main.pushremote\nother\0",
            "url.https://other.invalid/.pushinsteadof\nhttps://old.invalid/\0",
        ] {
            let mut changed = binding.clone();
            changed.configuration.push_str(config);
            assert!(binding.compare_snapshot(&changed).is_err());
        }
    }
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
        let mixed = AuthFailure::from_error(
            "remote: See https://docs.invalid/auth for help.\nfatal: Authentication failed for 'https://push-b.invalid/repo'",
            &candidates,
        );
        assert!(mixed.confirmed);
        assert_eq!(mixed.endpoints, ["https://push-b.invalid/repo"]);
        let help = AuthFailure::from_error(
            "remote: https://help.invalid/auth https://docs.invalid/auth\nAuthentication failed",
            &candidates,
        );
        assert!(!help.confirmed);
        assert_eq!(help.endpoints, candidates);
        let conflict = AuthFailure::from_error(
            "fatal: Authentication failed for 'https://push-a.invalid/repo'\nfatal: Authentication failed for 'https://push-b.invalid/repo'",
            &candidates,
        );
        assert!(!conflict.confirmed);
        assert_eq!(conflict.endpoints, candidates);
        let unknown = AuthFailure::from_error("Authentication failed", &candidates);
        assert!(!unknown.confirmed);
        assert_eq!(unknown.endpoints, candidates);
    }
}
