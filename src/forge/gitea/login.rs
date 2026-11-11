//! Cached login discovery and selected tea account credentials.
use super::*;

#[derive(Default)]
pub(super) struct Found {
    login: Option<Login>,
    at: Option<Instant>,
    refused: Vec<Login>,
}

/// How long "no login" stands before it's looked for again: a credential helper may ask for a
/// keyring to be unlocked, which nobody wants on every request.
pub(super) const NO_LOGIN_FOR: Duration = Duration::from_secs(10 * 60);

/// The logins found so far, by server.
pub(super) fn found_logins() -> &'static Mutex<HashMap<String, Found>> {
    static FOUND: OnceLock<Mutex<HashMap<String, Found>>> = OnceLock::new();
    FOUND.get_or_init(Default::default)
}

/// Sets the login kept for `web` aside: the server refused it, so it isn't sent there again and
/// the next request looks for another (a new `tea login`, a token stored in git's helper since), or
/// goes without.
pub(super) fn forget_login(web: &str) {
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
pub(super) fn login_key(web: &str) -> String {
    let c = crate::connections::current();
    format!("{web}\0{:?}\0{}", c.git_config, c.choices.tea_login)
}
pub(super) fn find_login(web: &str) -> Option<Login> {
    let Ok(mut found) = found_logins().lock() else {
        return None;
    };
    let entry = found.entry(login_key(web)).or_default();
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
                if !crate::connections::current().choices.tea_login.is_empty() {
                    return None;
                }
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

/// The login for `web` that needs no asking: one found before (a tab's, from git's credential
/// helper too), else the environment's or `tea`'s. Git's credential helper isn't run, so no
/// keyring asks to be unlocked; nothing is kept, so a tab still looks for itself.
pub(super) fn known_login(web: &str) -> Option<Login> {
    let found = found_logins().lock().ok()?;
    let entry = found.get(&login_key(web));
    if let Some(login) = entry.and_then(|e| e.login.clone()) {
        return Some(login);
    }
    let usable = |login: &Login| entry.is_none_or(|e| !e.refused.contains(login));
    env_login(web)
        .filter(usable)
        .or_else(|| tea_login(web).filter(usable))
}

/// `tea`'s variables, for the server they name only.
pub(super) fn env_login(web: &str) -> Option<Login> {
    if !crate::connections::current().choices.tea_login.is_empty() {
        return None;
    }
    let url = std::env::var("GITEA_SERVER_URL").ok()?;
    let token = std::env::var("GITEA_SERVER_TOKEN").ok()?;
    (same_server(&url, web) && !token.is_empty()).then_some(Login::Token(token))
}

pub(super) fn tea_login(web: &str) -> Option<Login> {
    let choices = crate::connections::current().choices;
    if !choices.tea_login.is_empty() {
        if !same_server(web, &format!("https://{}", choices.tea_host)) {
            return None;
        }
        return selected_tea_login(&tea_logins(), web, &choices.tea_login)
            .map(|l| Login::Token(l.token.clone()));
    }
    tea_logins()
        .into_iter()
        .find(|login| same_server(&login.url, web) && !login.token.is_empty())
        .map(|login| Login::Token(login.token))
}

pub(super) fn selected_tea_login<'a>(
    logins: &'a [TeaLogin],
    web: &str,
    alias: &str,
) -> Option<&'a TeaLogin> {
    logins.iter().find(|login| {
        login.name == alias && same_server(&login.url, web) && !login.token.is_empty()
    })
}

pub(crate) fn selected_tea_metadata(alias: &str) -> Option<(String, String)> {
    let logins = tea_logins();
    let login = logins.iter().find(|login| login.name == alias)?;
    let web = login.url.trim().trim_end_matches('/');
    let authority = web.strip_prefix("https://")?;
    if authority.is_empty() || authority.contains(['/', '@', '\n', '\r']) {
        return None;
    }
    Some((authority.to_owned(), login.user.clone()))
}

pub(crate) fn selected_tea_secret(alias: &str, host: &str) -> Result<(String, String), String> {
    let logins = tea_logins();
    let login = selected_tea_login(&logins, &format!("https://{host}"), alias).ok_or(
        crate::i18n::text("backend-the-selected-tea-login-is-unavailable-for-this-server-7ec179"),
    )?;
    if login.user.is_empty() {
        return Err(crate::i18n::text(
            "backend-the-tea-login-must-include-its-account-username-70a13b",
        )
        .to_owned());
    }
    Ok((login.user.clone(), login.token.clone()))
}

/// A login belongs to one web instance, including its case-sensitive proxy prefix.
pub(super) fn same_server(a: &str, b: &str) -> bool {
    super::super::identity::same_web_instance(a, b)
}

/// The servers `tea` is logged in to, as `(host, web address)`: a server whose name doesn't say
/// it's a Gitea or a Forgejo is still one when `tea` has a login there, and its login says where
/// its web pages are when the remote is an SSH one.
pub(in crate::forge) fn tea_servers() -> Vec<(String, String)> {
    tea_logins()
        .iter()
        .filter_map(|login| {
            let web = login.url.trim().trim_end_matches('/');
            let (host, _) = super::super::location(&format!("{web}/x/y"))?;
            Some((host, web.to_owned()))
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TeaLogin {
    pub(super) name: String,
    pub(super) user: String,
    pub(super) url: String,
    pub(super) token: String,
}

pub(super) fn tea_logins() -> Vec<TeaLogin> {
    tea_config_files()
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_tea_config(&text))
        .unwrap_or_default()
}

/// Where `tea` keeps its config: the user's config folder (as each system has it), or the old
/// place in the home folder.
pub(super) fn tea_config_files() -> Vec<PathBuf> {
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
pub(super) fn parse_tea_config(text: &str) -> Vec<TeaLogin> {
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
            "name" => login.name = value.to_owned(),
            "user" => login.user = value.to_owned(),
            "url" => login.url = value.to_owned(),
            "token" => login.token = value.to_owned(),
            _ => {}
        }
    }
    logins.extend(current);
    logins
}
