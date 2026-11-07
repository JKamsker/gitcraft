//! Forgejo and Gitea. Forgejo started as Gitea and keeps its API, so one module serves both: the
//! list of pull requests, opening one, the page of one in full ([`details`], [`post`]) and the
//! issues assigned to the user ([`issues`]), all through the REST API at `https://host/api/v1`.
//!
//! The login is one the user already has for the host, looked for in this order:
//! 1. `GITEA_SERVER_URL` and `GITEA_SERVER_TOKEN` in the environment (as `tea` reads them), when
//!    the URL is the host's;
//! 2. the logins of `tea`, Gitea's command-line tool (`tea login add`), which works with Forgejo
//!    too;
//! 3. what git's credential helper keeps for `https://host` (a password or an access token).
//!
//! Without one, public repositories can still be read.

mod details;
mod issues;
#[cfg(test)]
mod live;
#[cfg(test)]
mod live_issues;
mod post;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::http::{Api, Login};
use super::{Forge, Hosting, Issue, MergedPullRequest, NewPullRequest, Pages, Provider, PullRequest};
use crate::avatars::Hint;
use crate::pr_post::Target;
use crate::pull_request::{PrDetails, at};

pub(super) struct GiteaFamily {
    forge: Forge,
    name: &'static str,
    /// Hosts known to run it.
    hosts: &'static [&'static str],
    /// A word in a host's name that says it runs it (`forgejo.example.org`).
    word: &'static str,
}

pub(super) static FORGEJO: GiteaFamily = GiteaFamily {
    forge: Forge::Forgejo,
    name: "Forgejo",
    hosts: &["codeberg.org", "next.forgejo.org", "code.forgejo.org"],
    word: "forgejo",
};

pub(super) static GITEA: GiteaFamily = GiteaFamily {
    forge: Forge::Gitea,
    name: "Gitea",
    hosts: &["gitea.com"],
    word: "gitea",
};

impl GiteaFamily {
    fn api(&self, hosting: &Hosting) -> Api {
        Api::new(
            self.name,
            &hosting.web,
            &hosting.host,
            find_login(&hosting.web),
        )
        .on_refused(forget_login)
    }
}

impl Provider for GiteaFamily {
    fn name(&self) -> &'static str {
        self.name
    }

    fn claims(&self, host: &str) -> bool {
        self.hosts.contains(&host) || host.contains(self.word)
    }

    fn list(&self, _: &Path, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
        list(&self.api(hosting), hosting)
    }

    fn merged(&self, _: &Path, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
        merged(&self.api(hosting), hosting)
    }

    fn create(&self, _: &Path, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        create(&self.api(hosting), hosting, new)
    }

    fn assigned_issues(&self, _: &Path, hosting: &Hosting) -> Result<Vec<Issue>, String> {
        issues::assigned(&self.api(hosting), hosting)
    }

    fn issue(&self, _: &Path, hosting: &Hosting, number: u64) -> Result<Issue, String> {
        issues::one(&self.api(hosting), hosting, number)
    }

    /// The accounts the latest commits belong to, as the API ties them to their emails.
    fn avatars(&self, _: &Path, hosting: &Hosting, _: &[String]) -> Vec<(String, Hint)> {
        let endpoint = format!(
            "repos/{}/commits?limit=50&stat=false&verification=false&files=false",
            hosting.path
        );
        let Ok(answer) = self.api(hosting).get(&endpoint) else {
            return Vec::new();
        };
        super::parse_commit_avatars(&answer.to_string())
            .into_iter()
            .map(|(email, url)| (email, Hint::Url(url)))
            .collect()
    }

    fn pages(&self) -> Option<&dyn Pages> {
        Some(self)
    }
}

impl Pages for GiteaFamily {
    fn details(&self, _: &Path, hosting: &Hosting, number: u64) -> Result<PrDetails, String> {
        details::read(&self.api(hosting), self.forge, hosting, number)
    }

    fn target(&self, _: &Path, hosting: &Hosting, details: &PrDetails) -> Option<Box<dyn Target>> {
        post::GiteaTarget::of(self.api(hosting), self.name, hosting, details)
            .map(|t| Box::new(t) as Box<dyn Target>)
    }
}

fn list(api: &Api, hosting: &Hosting) -> Result<Vec<PullRequest>, String> {
    let endpoint = format!("repos/{}/pulls?state=open&sort=recentupdate", hosting.path);
    let items = api.get_all(&endpoint, 100)?;
    Ok(items.iter().map(parse_pull_request).collect())
}

/// The latest closed pull requests that were merged; the API lists those closed unmerged too.
fn merged(api: &Api, hosting: &Hosting) -> Result<Vec<MergedPullRequest>, String> {
    let endpoint = format!(
        "repos/{}/pulls?state=closed&sort=recentupdate",
        hosting.path
    );
    let items = api.get_all(&endpoint, 100)?;
    Ok(merged_only(&items))
}

fn merged_only(items: &[Value]) -> Vec<MergedPullRequest> {
    let merged: Vec<Value> = items
        .iter()
        .filter(|item| item.get("merged").and_then(Value::as_bool) == Some(true))
        .cloned()
        .collect();
    super::merged_items(&merged, ["/number", "/head/ref", "/head/sha", "/base/ref"])
}

fn create(api: &Api, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
    // Drafts are marked in the title, which both forges understand.
    let title = if new.draft && !is_draft_title(new.title) {
        format!("WIP: {}", new.title)
    } else {
        new.title.to_owned()
    };
    let payload = json!({
        "head": new.branch,
        "base": new.base,
        "title": title,
        "body": new.body,
    });
    let endpoint = format!("repos/{}/pulls", hosting.path);
    // A branch pushed a moment ago may not be known yet: the forge takes pushes in turn. Without
    // a login, "not found" is the private repository's answer, which waiting won't change.
    let mut tries = 0;
    let answer = loop {
        match api.send("POST", &endpoint, Some(&payload)) {
            Err(failure) if failure.status == Some(404) && api.logged_in() && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_secs(1));
            }
            answer => break answer?,
        }
    };
    let url = match at(&answer, "/html_url") {
        "" => {
            return Err(format!(
                "{} didn't say which pull request it opened",
                api.name()
            ));
        }
        url => url.to_owned(),
    };
    let number = answer.get("number").and_then(Value::as_u64).unwrap_or(0);
    if new.reviewers.is_empty() {
        return Ok(url);
    }
    if number == 0 {
        return Err(format!(
            "Opened {url}, but {} didn't say its number: ask for the reviews on its page",
            api.name()
        ));
    }
    let endpoint = format!("repos/{}/pulls/{number}/requested_reviewers", hosting.path);
    super::ask_reviewers(url, new.reviewers, |who| {
        api.send(
            "POST",
            &endpoint,
            Some(&reviewers_payload(&[who.to_owned()])),
        )
        .map(|_| ())
        .map_err(|e| e.message)
    })
}

/// Who to ask, as the API takes them: accounts, and teams by their name (the organisation is
/// the repository's).
fn reviewers_payload(reviewers: &[String]) -> Value {
    let (teams, users): (Vec<&String>, Vec<&String>) =
        reviewers.iter().partition(|r| r.contains('/'));
    let teams: Vec<&str> = teams
        .iter()
        .map(|t| t.rsplit('/').next().unwrap_or(t))
        .collect();
    json!({ "reviewers": users, "team_reviewers": teams })
}

/// Whether a title marks its pull request a draft (work in progress), as both forges read it.
fn is_draft_title(title: &str) -> bool {
    let title = title.trim_start().to_lowercase();
    ["wip:", "[wip]"].iter().any(|mark| title.starts_with(mark))
}

/// The account at `pointer`: a deleted one is "Ghost" to these forges.
fn user(v: &Value, pointer: &str) -> String {
    match at(v, pointer) {
        "" => "Ghost".to_owned(),
        name => name.to_owned(),
    }
}

fn parse_pull_request(item: &Value) -> PullRequest {
    let title = at(item, "/title");
    PullRequest {
        number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: title.to_owned(),
        branch: at(item, "/head/ref").to_owned(),
        sha: at(item, "/head/sha").to_owned(),
        author: user(item, "/user/login"),
        url: at(item, "/html_url").to_owned(),
        draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false) || is_draft_title(title),
    }
}

/// What was found for a server: the login, or that there's none, and the logins it refused.
#[derive(Default)]
struct Found {
    login: Option<Login>,
    at: Option<Instant>,
    refused: Vec<Login>,
}

/// How long "no login" stands before it's looked for again: a credential helper may ask for a
/// keyring to be unlocked, which nobody wants on every request.
const NO_LOGIN_FOR: Duration = Duration::from_secs(10 * 60);

/// The logins found so far, by server.
fn found_logins() -> &'static Mutex<HashMap<String, Found>> {
    static FOUND: OnceLock<Mutex<HashMap<String, Found>>> = OnceLock::new();
    FOUND.get_or_init(Default::default)
}

/// Sets the login kept for `web` aside: the server refused it, so it isn't sent there again and
/// the next request looks for another (a new `tea login`, a token stored in git's helper since), or
/// goes without.
fn forget_login(web: &str) {
    if let Ok(mut found) = found_logins().lock()
        && let Some(entry) = found.get_mut(web)
    {
        if let Some(login) = entry.login.take() {
            entry.refused.push(login);
        }
        entry.at = None;
    }
}

/// The login to send to the forge at `web` (`https://host[:port]`), if the user has one. What's
/// found is kept: a login until the server refuses it, the lack of one for a while. One lookup
/// runs at a time, so that a credential helper asks once.
fn find_login(web: &str) -> Option<Login> {
    let Ok(mut found) = found_logins().lock() else {
        return None;
    };
    let entry = found.entry(web.to_owned()).or_default();
    let fresh = match (&entry.login, entry.at) {
        (Some(_), _) => true,
        (None, Some(at)) => at.elapsed() < NO_LOGIN_FOR,
        (None, None) => false,
    };
    if !fresh {
        let refused = &entry.refused;
        let usable = |login: &Login| !refused.contains(login);
        let login = env_login(web)
            .filter(usable)
            .or_else(|| tea_login(web).filter(usable))
            .or_else(|| {
                crate::git::stored_credentials(web)
                    .map(|c| Login::Basic {
                        user: c.username,
                        password: c.password,
                    })
                    .filter(usable)
            });
        entry.login = login;
        entry.at = Some(Instant::now());
    }
    entry.login.clone()
}

/// `tea`'s variables, for the server they name only.
fn env_login(web: &str) -> Option<Login> {
    let url = std::env::var("GITEA_SERVER_URL").ok()?;
    let token = std::env::var("GITEA_SERVER_TOKEN").ok()?;
    (same_server(&url, web) && !token.is_empty()).then_some(Login::Token(token))
}

fn tea_login(web: &str) -> Option<Login> {
    tea_logins()
        .into_iter()
        .find(|login| same_server(&login.url, web) && !login.token.is_empty())
        .map(|login| Login::Token(login.token))
}

/// Whether two base URLs are the same server: scheme, host and port, whatever the case or a slash
/// at the end.
fn same_server(a: &str, b: &str) -> bool {
    let norm = |url: &str| url.trim().trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// The servers `tea` is logged in to, as `(host, web address)`: a server whose name doesn't say
/// it's a Gitea or a Forgejo is still one when `tea` has a login there, and its login says where
/// its web pages are when the remote is an SSH one.
pub(super) fn tea_servers() -> Vec<(String, String)> {
    tea_logins()
        .iter()
        .filter_map(|login| {
            let web = login.url.trim().trim_end_matches('/');
            let (host, _) = super::location(&format!("{web}/x/y"))?;
            Some((host, web.to_owned()))
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TeaLogin {
    url: String,
    token: String,
}

fn tea_logins() -> Vec<TeaLogin> {
    tea_config_files()
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_tea_config(&text))
        .unwrap_or_default()
}

/// Where `tea` keeps its config: the user's config folder (as each system has it), or the old
/// place in the home folder.
fn tea_config_files() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let home = var("HOME").or_else(|| var("USERPROFILE"));
    let mut files = Vec::new();
    if let Some(config) = var("XDG_CONFIG_HOME") {
        files.push(config.join("tea/config.yml"));
    }
    if let Some(home) = &home {
        files.push(home.join(".config/tea/config.yml"));
        files.push(home.join("Library/Application Support/tea/config.yml"));
    }
    for windows in ["LOCALAPPDATA", "APPDATA"] {
        if let Some(dir) = var(windows) {
            files.push(dir.join("tea").join("config.yml"));
        }
    }
    if let Some(home) = &home {
        files.push(home.join(".tea/tea.yml"));
    }
    files
}

/// The logins in `tea`'s config: a YAML list under `logins:`, each entry with a `url` and a
/// `token`. Only that much of YAML is read.
fn parse_tea_config(text: &str) -> Vec<TeaLogin> {
    let mut logins = Vec::new();
    let mut in_logins = false;
    let mut current: Option<TeaLogin> = None;
    for line in text.lines() {
        let content = line.trim_end();
        if content.trim().is_empty() || content.trim_start().starts_with('#') {
            continue;
        }
        if !content.starts_with([' ', '-']) {
            in_logins = content.trim() == "logins:";
            logins.extend(current.take());
            continue;
        }
        if !in_logins {
            continue;
        }
        let mut entry = content.trim_start();
        if let Some(rest) = entry.strip_prefix("- ").or_else(|| entry.strip_prefix('-')) {
            logins.extend(current.take());
            current = Some(TeaLogin::default());
            entry = rest.trim_start();
        }
        let (Some(login), Some((key, value))) = (current.as_mut(), entry.split_once(':')) else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        match key.trim() {
            "url" => login.url = value.to_owned(),
            "token" => login.token = value.to_owned(),
            _ => {}
        }
    }
    logins.extend(current);
    logins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_its_hosts() {
        assert!(FORGEJO.claims("codeberg.org"));
        assert!(FORGEJO.claims("forgejo.example.org"));
        assert!(!FORGEJO.claims("gitea.com"));
        assert!(GITEA.claims("gitea.com"));
        assert!(GITEA.claims("git.gitea.example"));
        assert!(!GITEA.claims("github.com"));
    }

    #[test]
    fn reads_the_list_of_pull_requests() {
        let item = json!({
            "number": 4, "title": "Fix it", "html_url": "https://codeberg.org/o/r/pulls/4",
            "head": {"ref": "fix", "sha": "abc"}, "user": {"login": "ana"}, "draft": false
        });
        let pr = parse_pull_request(&item);
        assert_eq!(
            (pr.number, pr.branch.as_str(), pr.author.as_str(), pr.draft),
            (4, "fix", "ana", false)
        );
        assert_eq!(pr.url, "https://codeberg.org/o/r/pulls/4");
        assert_eq!(pr.sha, "abc");
    }

    /// Closed and merged are told apart: only the merged ones say a branch's work is in.
    #[test]
    fn only_merged_pull_requests_count_as_merged() {
        let items = [
            json!({"number": 1, "head": {"ref": "a", "sha": "1"}, "base": {"ref": "main"}, "merged": true}),
            json!({"number": 2, "head": {"ref": "b", "sha": "2"}, "merged": false}),
            json!({"number": 3, "head": {"ref": "c", "sha": "3"}}),
        ];
        let merged = merged_only(&items);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            (
                merged[0].number,
                merged[0].branch.as_str(),
                merged[0].base.as_str()
            ),
            (1, "a", "main")
        );
        assert!(merged_only(&[]).is_empty());
    }

    #[test]
    fn a_work_in_progress_title_is_a_draft() {
        for title in ["WIP: x", "[WIP] x", "wip: x", "  [wip] x"] {
            assert!(is_draft_title(title), "{title}");
        }
        assert!(!is_draft_title("Wipe the cache"));
        let item = json!({"number": 1, "title": "[WIP] x", "user": null});
        let pr = parse_pull_request(&item);
        assert!(pr.draft);
        assert_eq!(pr.author, "Ghost");
    }

    #[test]
    fn reads_teas_logins() {
        let config = "logins:\n    - name: codeberg\n      url: https://codeberg.org\n      \
            token: \"abc123\"\n      default: true\n    - name: home\n      url: \
            'https://git.home.example:3000/'\n      token: def\n      user: me\npreferences:\n    \
            editor: false\n    url: https://not-a-login.example\n";
        let logins = parse_tea_config(config);
        assert_eq!(
            logins,
            [
                TeaLogin {
                    url: "https://codeberg.org".into(),
                    token: "abc123".into()
                },
                TeaLogin {
                    url: "https://git.home.example:3000/".into(),
                    token: "def".into()
                },
            ]
        );
    }

    #[test]
    fn teas_list_can_sit_flush_with_its_key() {
        let config = "logins:\n- name: a\n  url: https://a.example\n  token: t\n";
        assert_eq!(parse_tea_config(config)[0].token, "t");
        assert!(parse_tea_config("preferences:\n  editor: false\n").is_empty());
    }

    #[test]
    fn a_login_is_only_for_its_own_server() {
        assert!(same_server("https://Codeberg.org/", "https://codeberg.org"));
        assert!(!same_server(
            "https://codeberg.org",
            "https://codeberg.org:3000"
        ));
        assert!(!same_server("http://codeberg.org", "https://codeberg.org"));
        assert!(!same_server("https://evil.example", "https://codeberg.org"));
    }
}
