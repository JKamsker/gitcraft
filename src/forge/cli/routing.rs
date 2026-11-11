//! Bind forge CLI arguments to a validated captured repository and authority.
use super::*;

/// The command that runs `tool` with `args` about `repo` (here, or named for one on another
/// machine), and what to say when it can't be started.
pub(super) fn prepared(
    tool: &str,
    noun: &str,
    repo: &Path,
    hosting: &Hosting,
    args: &[&str],
) -> Result<(Command, impl Fn(String) -> String), String> {
    let mut cmd = tool_command(tool);
    let forge = forge_of(tool)?;
    if forge != hosting.forge || !plain_path(&hosting.path) || !plain_host(&hosting.host) {
        return Err(crate::i18n::message(
            "integration-forge-needs-selected",
            &[("tool", tool.into())],
        ));
    }
    let authority = match forge {
        Forge::GitLab => super::super::gitlab::api_authority(hosting),
        _ => hosting.host.clone(),
    };
    if !plain_host(&authority) || !same_host(&authority, &hosting.host) {
        return Err(crate::i18n::text("integration-forge-invalid-web-host").to_owned());
    }
    let remote = crate::remote::parse(repo).is_some();
    let context = crate::connections::current();
    if let Some(error) = &context.error {
        return Err(error.clone());
    }
    crate::connections::configure_cli(&mut cmd, tool, &authority)?;
    if remote {
        let remotes = remote_catalog(repo)?;
        validate_remote_target(&remotes, hosting)?;
        // A named target bypasses the CLI's usual known-host filtering. Only an explicitly
        // configured login (never an inherited global token) may authorize an SSH-tab host.
        if !trusted_host(forge, tool, &authority)? {
            return Err(not_signed_in(tool, &authority));
        }
    }
    if named_hosts(args)
        .iter()
        .any(|host| !host.eq_ignore_ascii_case(&authority))
    {
        return Err(crate::i18n::text("integration-forge-different-host").to_owned());
    }
    let mut args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let api = args.first().is_some_and(|s| s == "api");
    // These switches are command-family specific. Neither tool's `api` accepts --repo.
    // Insert before `--`, which release uploads use to protect asset filenames.
    let at = args.iter().position(|s| s == "--").unwrap_or(args.len());
    if api {
        if !args.iter().any(|s| s == "--hostname") {
            args.splice(at..at, ["--hostname".to_owned(), authority.clone()]);
        }
    } else {
        let target = match forge {
            Forge::GitLab => format!("{}/{}", hosting.web.trim_end_matches('/'), hosting.path),
            _ => format!("{}/{}", hosting.host, hosting.path),
        };
        if args.iter().any(|s| matches!(s.as_str(), "--repo" | "-R")) {
            return Err(crate::i18n::text("integration-forge-no-repository-override").to_owned());
        }
        args.splice(at..at, ["--repo".to_owned(), target]);
    }
    // Hosting and --repo identify the selected repository explicitly. Running inside
    // its worktree would let a local .glab-cli/config.yml redirect the CLI anyway.
    cmd.current_dir(std::env::temp_dir()).args(args);
    match forge {
        Forge::GitLab => {
            cmd.env("GITLAB_HOST", &authority)
                .env("GITLAB_API_HOST", &authority);
        }
        _ => {
            cmd.env("GH_REPO", format!("{}/{}", hosting.host, hosting.path));
        }
    }
    let here = remote || repo.is_dir();
    let (tool, noun, shown) = (tool.to_owned(), noun.to_owned(), repo.display().to_string());
    // Service failures are strings, not typed spawn errors. Keep the original cause and an
    // installation hint for a failed launch, without guessing an OS errno or rebuilding PATH.
    let start_error = move |error: String| {
        let id = if here && error.starts_with("could not run process:") {
            "backend-forge-launch-install"
        } else {
            "backend-forge-launch-error"
        };
        let purpose = match noun.as_str() {
            "pull request" => crate::i18n::text("backend-forge-purpose-pull"),
            "merge request" => crate::i18n::text("backend-forge-purpose-merge"),
            "issue" => crate::i18n::text("backend-forge-purpose-issue"),
            other => other,
        };
        crate::i18n::message(
            id,
            &[
                ("tool", tool.as_str().into()),
                ("path", shown.as_str().into()),
                ("detail", error.into()),
                ("purpose", purpose.into()),
            ],
        )
    };
    Ok((cmd, start_error))
}

/// The forge whose tool `tool` is.
pub(super) fn forge_of(tool: &str) -> Result<Forge, String> {
    match tool {
        "gh" => Ok(Forge::GitHub),
        "glab" => Ok(Forge::GitLab),
        _ => Err(crate::i18n::message(
            "backend-tool-can-t-be-run-for-a-repository-on-another-machine-966b8b",
            &[("tool", (tool).to_string().into())],
        )),
    }
}

/// Whether `glab` with `args` takes `--repo`: every command but `api`.
#[cfg(test)]
pub(super) fn gitlab_takes_repo(args: &[&str]) -> bool {
    args.first().is_some_and(|sub| *sub != "api")
}

/// Re-read the SSH repository's catalog before each command; selection is a host AND path,
/// rather than an implicit origin. A changed/removed URL requires refreshing the tab.
pub(super) fn remote_catalog(repo: &Path) -> Result<Vec<crate::git::Remote>, String> {
    #[cfg(test)]
    if let Some(remotes) = recording::remotes() {
        return Ok(remotes);
    }
    crate::git::remotes(repo)
}

pub(super) fn validate_remote_target(
    remotes: &[crate::git::Remote],
    hosting: &Hosting,
) -> Result<(), String> {
    if remotes
        .iter()
        .any(|remote| hosting.matches_git_remote(&remote.url))
    {
        Ok(())
    } else {
        Err(crate::i18n::text("integration-forge-remote-changed").to_owned())
    }
}

pub(super) fn plain_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.', ':'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

// The inbox's retry policy consumes this protocol identity; translate at display_error.
pub(super) fn not_signed_in(tool: &str, host: &str) -> String {
    format!("{tool} isn't signed in to {host}: `{tool} auth login --hostname {host}` to use it")
}

/// Whether `path` is a plain `owner/repo` (`group/sub/repo`): never a URL, an `scp`-style
/// address or an option, which `glab --repo` would take as another host.
pub(super) fn plain_path(path: &str) -> bool {
    path.contains('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !matches!(part, "." | "..")
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// The hosts `args` name: the value of `--hostname`, and the host of a `-R` / `--repo` of the
/// form `host/owner/repo`.
pub(super) fn named_hosts(args: &[&str]) -> Vec<String> {
    let mut hosts = Vec::new();
    for pair in args.windows(2) {
        match pair[0] {
            "--hostname" => hosts.push(pair[1].to_owned()),
            "-R" | "--repo" if pair[1].matches('/').count() >= 2 => {
                hosts.push(pair[1].split('/').next().unwrap_or_default().to_owned());
            }
            _ => {}
        }
    }
    hosts
}

/// Whether two hosts are the same, ports aside (`glab api --hostname` takes none).
pub(super) fn same_host(a: &str, b: &str) -> bool {
    let name = |h: &str| h.split(':').next().unwrap_or(h).to_lowercase();
    name(a) == name(b)
}

/// The host and `owner/repo` the tool is given for remote `url`: GitLab's host with the port of
/// an `http(s)` URL, an SSH port left out (it says nothing of the web pages).
pub(crate) fn tool_location(forge: Forge, url: &str) -> Option<(String, String)> {
    let (host, path) = super::super::location(url)?;
    let host = match (forge, http_port(url)) {
        (Forge::GitLab, Some(port)) => format!("{host}:{port}"),
        _ => host,
    };
    Some((host, path))
}

/// The port of an `http(s)` URL that names one.
pub(super) fn http_port(url: &str) -> Option<&str> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (_, port) = host.rsplit_once(':')?;
    (!port.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).then_some(port)
}
