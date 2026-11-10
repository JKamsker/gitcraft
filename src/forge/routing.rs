//! Forge detection, captured remote routing, and web-location parsing.
use super::*;

/// The forge hosting usable `origin` (or the first usable remote). git's config can say which it is, for a
/// server whose name doesn't tell: `gitcraft.forge` for one repository, `gitcraft.<host>.forge`
/// for every repository on that host (`git config --global gitcraft.git.example.org.forge
/// forgejo`). Otherwise the host's name decides, then the servers `tea` is logged in to, then
/// the remote's whole URL.
pub fn detect(repo: &Path, remotes: &[Remote]) -> Option<Hosting> {
    let hosts = detect_remotes(repo, remotes);
    hosts
        .iter()
        .find(|(name, _)| name == "origin")
        .or_else(|| hosts.first())
        .map(|(_, hosting)| hosting.clone())
}

/// [`detect`], given git's config and the servers `tea` knows as `(host, web address)`.
pub(super) fn detect_with(
    remotes: &[Remote],
    config: &HashMap<String, String>,
    tea: &[(String, String)],
) -> Option<Hosting> {
    remotes
        .iter()
        .find(|r| r.name == "origin")
        .and_then(|r| detect_remote(r, config, tea))
        .or_else(|| remotes.iter().find_map(|r| detect_remote(r, config, tea)))
}

/// All usable forge remotes, detected locally even while offline. A local/unrecognized
/// `origin` must not prevent another usable remote from being selected.
pub(crate) fn detect_remotes(repo: &Path, remotes: &[Remote]) -> Vec<(String, Hosting)> {
    let config = gitcraft_config(repo);
    let tea = gitea::tea_servers();
    remotes
        .iter()
        .filter_map(|remote| Some((remote.name.clone(), detect_remote(remote, &config, &tea)?)))
        .collect()
}

pub(super) fn detect_remote(
    remote: &Remote,
    config: &HashMap<String, String>,
    tea: &[(String, String)],
) -> Option<Hosting> {
    let tea_web = |host: &str| {
        tea.iter()
            .find(|(h, _)| h == host)
            .map(|(_, web)| web.clone())
    };
    // A local path has no host, and `C:/…` only a drive's: neither is on any forge, whatever
    // folder names it goes through.
    let (host, path) = location(&remote.url).filter(|(host, _)| host.len() > 1)?;
    // Gerrit has its own change-based review flow. An explicit Gerrit override
    // must not fall through to GitHub/GitLab host guessing for this remote.
    if config
        .get("gitcraft.forge")
        .or_else(|| config.get(&format!("gitcraft.{host}.forge")))
        .is_some_and(|kind| kind.eq_ignore_ascii_case("gerrit"))
    {
        return None;
    }
    let forge = choose(&host, config)
        .or_else(|| tea_web(&host).map(|_| Forge::Gitea))
        // GitLab can also live under a path of another host (`https://example.org/gitlab/…`) or
        // be reached as `gitlab@…`. GitHub goes by its host alone: a path naming it says nothing.
        .or_else(|| {
            remote
                .url
                .to_lowercase()
                .contains("gitlab")
                .then_some(Forge::GitLab)
        })?;
    // An SSH remote says nothing of the web server; `tea`'s login for the host does.
    let web = if forge == Forge::Azure {
        "https://dev.azure.com".into()
    } else {
        match web_base(&remote.url) {
            Some(web) => web,
            None => tea_web(&host).unwrap_or_else(|| format!("https://{host}")),
        }
    };
    Some(Hosting {
        forge,
        web,
        host,
        path,
    })
}

/// The forge for `host`: from git's config if it names one, else from the host's name.
pub(super) fn choose(host: &str, config: &HashMap<String, String>) -> Option<Forge> {
    let configured = |key: &str| config.get(key).and_then(|value| Forge::from_key(value));
    configured("gitcraft.forge")
        .or_else(|| configured(&format!("gitcraft.{host}.forge")))
        .or_else(|| {
            Forge::ALL
                .into_iter()
                .find(|forge| forge.provider().claims(host))
        })
}

/// gitcraft's own settings in git's config, keys lowercase. For a repository on another
/// machine, also this machine's global config, where the README says to set them: that
/// machine's (and the repository's own) win.
pub(crate) fn gitcraft_config(repo: &Path) -> HashMap<String, String> {
    // Exit code 1: nothing set, which is the usual.
    let read = |dir: Option<&Path>, scope: &[&str]| {
        let mut args = vec!["config"];
        args.extend(scope);
        args.extend(["--get-regexp", r"^gitcraft\."]);
        parse_config(&crate::git::run_ok_codes(dir, &args, &[0, 1]).unwrap_or_default())
    };
    let mut config = if crate::remote::is_remote(repo) {
        read(None, &["--global"])
    } else {
        HashMap::new()
    };
    config.extend(read(Some(repo), &[]));
    config
}

pub(super) fn parse_config(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(' ')?;
            Some((key.to_lowercase(), value.trim().to_owned()))
        })
        .collect()
}

/// `https://host[:port]` of a web remote; nothing for an SSH one, whose port says nothing about
/// the web server's.
pub(super) fn web_base(url: &str) -> Option<String> {
    let url = url.trim();
    ["https", "http"].into_iter().find_map(|scheme| {
        let rest = url.strip_prefix(&format!("{scheme}://"))?;
        let authority = rest.split('/').next().unwrap_or(rest);
        let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
        Some(format!("{scheme}://{}", authority.to_lowercase()))
    })
}

/// The host of `host[:port]`, an IPv6 one (`[fd00::5]:3000`) with its brackets.
pub(super) fn without_port(authority: &str) -> &str {
    match authority.find(']') {
        Some(end) if authority.starts_with('[') => &authority[..=end],
        _ => authority.split(':').next().unwrap_or(authority),
    }
}

/// The web host and `owner/repo` path of a remote URL: `https://host/owner/repo.git`,
/// `git@host:owner/repo.git` or `ssh://git@host[:port]/owner/repo`.
pub fn location(url: &str) -> Option<(String, String)> {
    if let Some(location) = azure::location(url) {
        return Some(location);
    }
    let url = url.trim();
    let (host, path) = match url.split_once("://") {
        Some((_, rest)) => {
            // The user part is only ever before the first `/`: an `@` in the path names no host.
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            (without_port(host), path)
        }
        None => {
            let rest = url.rsplit_once('@').map_or(url, |(_, r)| r);
            rest.split_once(':')?
        }
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || !path.contains('/') {
        return None;
    }
    // SSH over the HTTPS port goes to another name than the forge's own.
    let host = match host.to_lowercase().as_str() {
        "ssh.github.com" => "github.com".to_owned(),
        "altssh.gitlab.com" => "gitlab.com".to_owned(),
        host => host.to_owned(),
    };
    Some((host, path.to_owned()))
}

/// The web page of the repository at remote `url`: `https://github.com/owner/repo` for
/// `git@github.com:owner/repo.git`. `None` for a local path, which has no page.
pub fn web_url(url: &str) -> Option<String> {
    if let Some((host, path)) = azure::location(url) {
        return Some(format!(
            "https://{host}/{}",
            crate::pr_post::encode_path(&path)
        ));
    }
    let (host, path) = location(url)?;
    // `C:/repos/x` reads as host `c`; a host that serves pages has a domain.
    if !host.contains('.') {
        return None;
    }
    // A web remote names its server, port included; an SSH remote's port is not the web's.
    let base = web_base(&url.to_lowercase()).unwrap_or_else(|| format!("https://{host}"));
    Some(format!("{base}/{path}"))
}
