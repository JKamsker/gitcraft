//! The forges a repository can be hosted on — GitHub, GitLab, Forgejo, Gitea — behind one
//! interface. Each has a module of its own that implements [`Provider`]; the rest of the app only
//! talks to that, so adding a forge means adding a module and a [`Forge`] variant.
//!
//! - GitHub and GitLab go through their own command-line tools, `gh` and `glab`. They hold the
//!   user's login, so gitcraft never stores a token.
//! - Forgejo and Gitea share one API, which gitcraft calls itself with the login `tea` (or git's
//!   credential helper) already has for the host. See [`gitea`].

mod cli;
pub(crate) use cli::tool_location;
mod gitea;
mod github;
mod gitlab;
mod http;
pub mod links;

use std::collections::HashMap;
use std::path::Path;

use crate::avatars::Hint;
use crate::git::Remote;
use crate::pr_post::Target;
use crate::pull_request::PrDetails;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Forge {
    GitHub,
    GitLab,
    Forgejo,
    Gitea,
}

impl Forge {
    pub const ALL: [Forge; 4] = [Forge::GitHub, Forge::GitLab, Forge::Forgejo, Forge::Gitea];

    /// The module that does the work.
    pub fn provider(self) -> &'static dyn Provider {
        match self {
            Forge::GitHub => &github::GitHub,
            Forge::GitLab => &gitlab::GitLab,
            Forge::Forgejo => &gitea::FORGEJO,
            Forge::Gitea => &gitea::GITEA,
        }
    }

    /// "GitHub", "Forgejo"…
    pub fn name(self) -> &'static str {
        self.provider().name()
    }

    /// "pull request" or "merge request".
    pub fn noun(self) -> &'static str {
        self.provider().noun()
    }

    /// The heading of the sidebar's list.
    pub fn list_title(self) -> &'static str {
        match self.noun() {
            "merge request" => "MERGE REQUESTS",
            _ => "PULL REQUESTS",
        }
    }

    /// Whether gitcraft can show its pull requests in full (and review and merge them).
    pub fn has_pages(self) -> bool {
        self.provider().pages().is_some()
    }

    /// The name `gitcraft.forge` is set to in git's config to pick this forge.
    pub fn key(self) -> &'static str {
        match self {
            Forge::GitHub => "github",
            Forge::GitLab => "gitlab",
            Forge::Forgejo => "forgejo",
            Forge::Gitea => "gitea",
        }
    }

    pub fn from_key(key: &str) -> Option<Forge> {
        let key = key.trim().to_lowercase();
        Forge::ALL.into_iter().find(|forge| forge.key() == key)
    }
}

/// Where a repository lives: which forge, and where on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hosting {
    pub forge: Forge,
    /// The host's name, lowercase and without a port: what pull requests' URLs are matched by.
    pub host: String,
    /// `owner/repo` (on GitLab, `group/sub/repo`).
    pub path: String,
    /// Where its web pages and API are: `https://host`, with the port when the remote names one.
    pub web: String,
}

/// What a new pull request is made of.
pub struct NewPullRequest<'a> {
    pub branch: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// The branch it proposes (head / source branch).
    pub branch: String,
    /// Its latest commit.
    pub sha: String,
    pub author: String,
    pub url: String,
    pub draft: bool,
}

/// A merged pull request: what branch cleanup needs of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedPullRequest {
    pub number: u64,
    /// The branch it proposed.
    pub branch: String,
    /// Its last commit.
    pub sha: String,
    /// The branch it was merged into.
    pub base: String,
}

/// The merged pull requests in a forge's JSON list, `pointers` saying where its number, branch,
/// last commit and base are in each item.
fn parse_merged(
    json: &str,
    pointers: [&str; 4],
    tool: &str,
) -> Result<Vec<MergedPullRequest>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected {tool} output: {e}"))?;
    Ok(merged_items(
        value.as_array().map_or(&[][..], Vec::as_slice),
        pointers,
    ))
}

/// [`parse_merged`] of items already read.
fn merged_items(items: &[serde_json::Value], pointers: [&str; 4]) -> Vec<MergedPullRequest> {
    let [number, branch, sha, base] = pointers;
    let text = |v: &serde_json::Value, p: &str| {
        v.pointer(p)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    items
        .iter()
        .map(|item| MergedPullRequest {
            number: item
                .pointer(number)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            branch: text(item, branch),
            sha: text(item, sha),
            base: text(item, base),
        })
        .filter(|pr| !pr.branch.is_empty() && !pr.sha.is_empty())
        .collect()
}

/// What a forge does for gitcraft.
pub trait Provider: Sync {
    /// "GitHub", "Forgejo"…
    fn name(&self) -> &'static str;

    /// What it calls a proposed change.
    fn noun(&self) -> &'static str {
        "pull request"
    }

    /// Whether a host is this forge's by its name alone (`github.com`, `gitlab.example.org`).
    fn claims(&self, host: &str) -> bool;

    /// The open pull requests.
    fn list(&self, repo: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String>;

    /// The latest pull requests merged (up to 100): a local branch still at or behind the last
    /// commit of one it proposed was merged, even squashed or rebased into other commits.
    fn merged(&self, _repo: &Path, _hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        Ok(Vec::new())
    }

    /// Opens a pull request; returns its URL (or whatever the forge said).
    fn create(
        &self,
        repo: &Path,
        hosting: &Hosting,
        new: &NewPullRequest,
    ) -> Result<String, String>;

    /// Where the account pictures of the authors (by commit email) are.
    fn avatars(&self, repo: &Path, hosting: &Hosting, emails: &[String]) -> Vec<(String, Hint)>;

    /// The page of one pull request, if gitcraft has one for this forge.
    fn pages(&self) -> Option<&dyn Pages> {
        None
    }
}

/// One pull request in full: reading it, and acting on it.
pub trait Pages: Sync {
    /// Reads the pull request numbered `number`: description, checks, commits, conversation,
    /// threads on lines and what the person logged in may do.
    fn details(&self, repo: &Path, hosting: &Hosting, number: u64) -> Result<PrDetails, String>;

    /// What says or does something on the pull request `details`.
    fn target(
        &self,
        repo: &Path,
        hosting: &Hosting,
        details: &PrDetails,
    ) -> Option<Box<dyn Target>>;
}

/// The forge hosting `origin` (or the first remote). git's config can say which it is, for a
/// server whose name doesn't tell: `gitcraft.forge` for one repository, `gitcraft.<host>.forge`
/// for every repository on that host (`git config --global gitcraft.git.example.org.forge
/// forgejo`). Otherwise the host's name decides, then the servers `tea` is logged in to, then
/// the remote's whole URL.
pub fn detect(repo: &Path, remotes: &[Remote]) -> Option<Hosting> {
    detect_with(remotes, &gitcraft_config(repo), &gitea::tea_servers())
}

/// [`detect`], given git's config and the servers `tea` knows as `(host, web address)`.
fn detect_with(
    remotes: &[Remote],
    config: &HashMap<String, String>,
    tea: &[(String, String)],
) -> Option<Hosting> {
    let remote = remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| remotes.first())?;
    let tea_web = |host: &str| {
        tea.iter()
            .find(|(h, _)| h == host)
            .map(|(_, web)| web.clone())
    };
    // A local path has no host, and `C:/…` only a drive's: neither is on any forge, whatever
    // folder names it goes through.
    let (host, path) = location(&remote.url).filter(|(host, _)| host.len() > 1)?;
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
    let web = match web_base(&remote.url) {
        Some(web) => web,
        None => tea_web(&host).unwrap_or_else(|| format!("https://{host}")),
    };
    Some(Hosting {
        forge,
        web,
        host,
        path,
    })
}

/// The forge for `host`: from git's config if it names one, else from the host's name.
fn choose(host: &str, config: &HashMap<String, String>) -> Option<Forge> {
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
fn gitcraft_config(repo: &Path) -> HashMap<String, String> {
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

fn parse_config(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(' ')?;
            Some((key.to_lowercase(), value.trim().to_owned()))
        })
        .collect()
}

/// `https://host[:port]` of a web remote; nothing for an SSH one, whose port says nothing about
/// the web server's.
fn web_base(url: &str) -> Option<String> {
    let url = url.trim();
    ["https", "http"].into_iter().find_map(|scheme| {
        let rest = url.strip_prefix(&format!("{scheme}://"))?;
        let authority = rest.split('/').next().unwrap_or(rest);
        let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
        Some(format!("{scheme}://{}", authority.to_lowercase()))
    })
}

/// The host of `host[:port]`, an IPv6 one (`[fd00::5]:3000`) with its brackets.
fn without_port(authority: &str) -> &str {
    match authority.find(']') {
        Some(end) if authority.starts_with('[') => &authority[..=end],
        _ => authority.split(':').next().unwrap_or(authority),
    }
}

/// The web host and `owner/repo` path of a remote URL: `https://host/owner/repo.git`,
/// `git@host:owner/repo.git` or `ssh://git@host[:port]/owner/repo`.
pub fn location(url: &str) -> Option<(String, String)> {
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
    let (host, path) = location(url)?;
    // `C:/repos/x` reads as host `c`; a host that serves pages has a domain.
    if !host.contains('.') {
        return None;
    }
    // A web remote names its server, port included; an SSH remote's port is not the web's.
    let base = web_base(&url.to_lowercase()).unwrap_or_else(|| format!("https://{host}"));
    Some(format!("{base}/{path}"))
}

/// Runs the command-line tool of `forge` (`gh`, `glab`) in `repo`, for what goes past pull
/// requests (builds). Forgejo and Gitea have none gitcraft uses.
pub(crate) fn tool(forge: Forge, repo: &Path, args: &[&str]) -> Result<String, String> {
    let tool = match forge {
        Forge::GitHub => "gh",
        Forge::GitLab => "glab",
        Forge::Forgejo | Forge::Gitea => {
            return Err(format!("gitcraft has no tool for {}", forge.name()));
        }
    };
    cli::run(tool, forge.noun(), repo, args)
}

/// Account pictures by commit email, from a list of commits as GitHub's, Forgejo's and Gitea's
/// APIs give it: each commit is tied to the accounts its emails belong to.
fn parse_commit_avatars(json: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else {
        return out;
    };
    for item in &items {
        for (who, account) in [("author", "author"), ("committer", "committer")] {
            let email = item
                .pointer(&format!("/commit/{who}/email"))
                .and_then(|e| e.as_str());
            let avatar = item
                .pointer(&format!("/{account}/avatar_url"))
                .and_then(|a| a.as_str())
                .filter(|a| !a.is_empty());
            if let (Some(email), Some(avatar)) = (email, avatar) {
                out.entry(email.trim().to_lowercase())
                    .or_insert_with(|| avatar.to_owned());
            }
        }
    }
    out
}

#[cfg(test)]
mod merged_tests {
    use super::*;

    #[test]
    fn merged_lists_are_read_with_their_base() {
        let json = r#"[{"number": 5, "headRefName": "fix/ü", "headRefOid": "abc", "baseRefName": "main"},
                       {"number": 6, "headRefName": "", "headRefOid": "def", "baseRefName": "main"}]"#;
        let read = parse_merged(
            json,
            ["/number", "/headRefName", "/headRefOid", "/baseRefName"],
            "gh",
        );
        assert_eq!(
            read,
            Ok(vec![MergedPullRequest {
                number: 5,
                branch: "fix/ü".to_owned(),
                sha: "abc".to_owned(),
                base: "main".to_owned(),
            }])
        );
        assert_eq!(parse_merged("[]", ["", "", "", ""], "gh"), Ok(Vec::new()));
        assert!(parse_merged("not json", ["", "", "", ""], "gh").is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_locations() {
        let gh = Some(("github.com".to_owned(), "owner/repo".to_owned()));
        assert_eq!(location("https://github.com/owner/repo.git"), gh);
        assert_eq!(location("https://user@github.com/owner/repo/"), gh);
        assert_eq!(location("git@github.com:owner/repo.git"), gh);
        assert_eq!(location("ssh://git@github.com:22/owner/repo"), gh);
        assert_eq!(
            location("https://gitlab.example.org/group/sub/proj.git"),
            Some(("gitlab.example.org".to_owned(), "group/sub/proj".to_owned()))
        );
        assert_eq!(location("/srv/git/repo.git"), None);
        // GitHub's and GitLab's SSH over port 443.
        assert_eq!(location("ssh://git@ssh.github.com:443/owner/repo.git"), gh);
        assert_eq!(
            location("ssh://git@altssh.gitlab.com:443/g/p.git"),
            Some(("gitlab.com".to_owned(), "g/p".to_owned()))
        );
    }

    #[test]
    fn web_pages_of_remotes() {
        let page = Some("https://github.com/JKamsker/gitcraft".to_owned());
        assert_eq!(web_url("https://github.com/JKamsker/gitcraft.git"), page);
        assert_eq!(web_url("git@github.com:JKamsker/gitcraft.git"), page);
        assert_eq!(web_url("ssh://git@github.com:22/JKamsker/gitcraft"), page);
        assert_eq!(
            web_url("http://git.example.org/team/repo.git"),
            Some("http://git.example.org/team/repo".to_owned())
        );
        assert_eq!(
            web_url("https://git.example.org:8443/team/repo.git"),
            Some("https://git.example.org:8443/team/repo".to_owned())
        );
        assert_eq!(
            web_url("ssh://git@git.example.org:2222/team/repo.git"),
            Some("https://git.example.org/team/repo".to_owned())
        );
        assert_eq!(
            web_url("https://me@git.example.org:3000/o/r.git"),
            Some("https://git.example.org:3000/o/r".to_owned())
        );
        assert_eq!(web_url("C:/repos/gitcraft"), None);
        assert_eq!(web_url("/srv/git/repo.git"), None);
    }

    #[test]
    fn avatars_by_email() {
        let json = r#"[
            {"commit": {"author": {"email": "Ann@x.org"}, "committer": {"email": "noreply@github.com"}},
             "author": {"avatar_url": "https://a/1?v=4"}, "committer": {"avatar_url": "https://a/web?v=4"}},
            {"commit": {"author": {"email": "nobody@x.org"}, "committer": {"email": "nobody@x.org"}},
             "author": null, "committer": null}
        ]"#;
        let m = parse_commit_avatars(json);
        assert_eq!(
            m.get("ann@x.org").map(String::as_str),
            Some("https://a/1?v=4")
        );
        assert!(m.contains_key("noreply@github.com"));
        assert!(!m.contains_key("nobody@x.org"));
    }

    #[test]
    fn detects_forge_from_the_host() {
        let none = HashMap::new();
        assert_eq!(choose("github.com", &none), Some(Forge::GitHub));
        assert_eq!(choose("github.example.com", &none), Some(Forge::GitHub));
        assert_eq!(choose("gitlab.com", &none), Some(Forge::GitLab));
        assert_eq!(choose("codeberg.org", &none), Some(Forge::Forgejo));
        assert_eq!(choose("forgejo.example.org", &none), Some(Forge::Forgejo));
        assert_eq!(choose("gitea.com", &none), Some(Forge::Gitea));
        assert_eq!(choose("git.example.org", &none), None);
        assert_eq!(choose("c", &none), None);
    }

    #[test]
    fn detects_forge_from_origin() {
        let r = |name: &str, url: &str| Remote {
            name: name.into(),
            url: url.into(),
        };
        // Neither the machine's git config nor its `tea` logins: what the remotes say alone.
        let forge =
            |remotes: &[Remote]| detect_with(remotes, &HashMap::new(), &[]).map(|h| h.forge);
        assert_eq!(
            forge(&[r("origin", "git@github.com:a/b.git")]),
            Some(Forge::GitHub)
        );
        assert_eq!(
            forge(&[
                r("up", "https://github.com/x/y"),
                r("origin", "https://gitlab.com/a/b")
            ]),
            Some(Forge::GitLab)
        );
        assert_eq!(
            forge(&[r("origin", "ssh://git@codeberg.org/a/b.git")]),
            Some(Forge::Forgejo)
        );
        assert_eq!(forge(&[r("origin", "C:/repos/bare.git")]), None);
        // A folder's name is no forge.
        assert_eq!(forge(&[r("origin", "/srv/gitlab-backup/x.git")]), None);
        assert_eq!(forge(&[r("origin", "C:/gitlab/x.git")]), None);
        assert_eq!(
            forge(&[r("origin", "https://github.example.com/a/b")]),
            Some(Forge::GitHub)
        );
    }

    #[test]
    fn only_the_host_tells_the_forge() {
        let r = |url: &str| Remote {
            name: "origin".into(),
            url: url.into(),
        };
        let forge =
            |remotes: &[Remote]| detect_with(remotes, &HashMap::new(), &[]).map(|h| h.forge);
        assert_eq!(
            forge(&[r("git@gitlab.com:me/me.github.io.git")]),
            Some(Forge::GitLab)
        );
        assert_eq!(forge(&[r("https://git.example.org/github.com/x")]), None);
        assert_eq!(forge(&[r("https://git.example.org/a@github.com/x")]), None);
        assert_eq!(
            forge(&[r("https://github-mirror.gitlab.example.org/g/p.git")]),
            Some(Forge::GitLab)
        );
        assert_eq!(
            location("https://me:tok@github.com/a/b.git"),
            Some(("github.com".into(), "a/b".into()))
        );
        assert_eq!(
            forge(&[r("https://example.org/gitlab/x/y")]),
            Some(Forge::GitLab)
        );
    }

    #[test]
    fn git_config_names_the_forge_of_a_host_its_name_doesnt_tell() {
        let config = parse_config(
            "gitcraft.git.example.org.forge forgejo\ngitcraft.other.example.org.forge Gitea\n",
        );
        assert_eq!(choose("git.example.org", &config), Some(Forge::Forgejo));
        assert_eq!(choose("other.example.org", &config), Some(Forge::Gitea));
        assert_eq!(choose("third.example.org", &config), None);
    }

    #[test]
    fn a_repositorys_own_setting_wins() {
        let config = parse_config("gitcraft.forge gitea\ngitcraft.github.com.forge github\n");
        assert_eq!(choose("github.com", &config), Some(Forge::Gitea));
        let nonsense = parse_config("gitcraft.forge bitbucket\n");
        assert_eq!(choose("github.com", &nonsense), Some(Forge::GitHub));
    }

    #[test]
    fn a_remote_whose_host_doesnt_tell_goes_by_its_whole_url() {
        let r = |url: &str| Remote {
            name: "origin".into(),
            url: url.into(),
        };
        let forge = |url: &str| detect_with(&[r(url)], &HashMap::new(), &[]).map(|h| h.forge);
        assert_eq!(
            forge("gitlab@code.acme.com:group/proj.git"),
            Some(Forge::GitLab)
        );
        assert_eq!(
            forge("https://acme.com/gitlab/group/proj.git"),
            Some(Forge::GitLab)
        );
        // A local path has no host: whatever its folders are called, it's on no forge.
        assert_eq!(forge("/home/me/src/github.com/o/r"), None);
        // A host that says what it is wins over the rest of the URL.
        assert_eq!(
            forge("https://gitlab.com/ana/ana.github.io"),
            Some(Forge::GitLab)
        );
    }

    #[test]
    fn an_ssh_remote_gets_its_web_address_from_teas_login() {
        let r = Remote {
            name: "origin".into(),
            url: "git@gitea.lan:o/r.git".into(),
        };
        let tea = [("gitea.lan".to_owned(), "http://gitea.lan:3000".to_owned())];
        let hosting = detect_with(&[r], &HashMap::new(), &tea);
        assert_eq!(
            hosting.map(|h| (h.forge, h.web)),
            Some((Forge::Gitea, "http://gitea.lan:3000".to_owned()))
        );
    }

    #[test]
    fn the_web_server_keeps_its_port_and_an_ssh_one_is_dropped() {
        assert_eq!(
            web_base("https://me:tok@Git.Example.org:3000/o/r.git").as_deref(),
            Some("https://git.example.org:3000")
        );
        assert_eq!(
            web_base("http://git.local/o/r.git").as_deref(),
            Some("http://git.local")
        );
        assert_eq!(web_base("ssh://git@git.example.org:2222/o/r.git"), None);
        assert_eq!(web_base("git@codeberg.org:o/r.git"), None);
        let r = Remote {
            name: "origin".into(),
            url: "ssh://git@git.example.org:2222/o/r.git".into(),
        };
        let config = parse_config("gitcraft.forge forgejo\n");
        let hosting = detect_with(&[r], &config, &[]);
        assert_eq!(
            hosting.map(|h| h.web).as_deref(),
            Some("https://git.example.org")
        );
    }

    #[test]
    fn an_ipv6_host_keeps_its_brackets() {
        assert_eq!(
            location("https://[fd00::5]:3000/o/r.git"),
            Some(("[fd00::5]".to_owned(), "o/r".to_owned()))
        );
        assert_eq!(
            web_base("https://[fd00::5]:3000/o/r.git").as_deref(),
            Some("https://[fd00::5]:3000")
        );
    }
}
