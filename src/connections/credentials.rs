//! Resolve and verify selected CLI accounts without exposing credentials.
use super::*;

pub(super) struct Secret {
    pub(super) value: String,
    pub(super) oauth: bool,
}

pub(super) const GH_TOKENS: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

pub(super) const GL_TOKENS: &[&str] = &[
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "OAUTH_TOKEN",
    "GLAB_TOKEN",
    "CI_JOB_TOKEN",
    "JOB_TOKEN",
    "OAUTH2_REFRESH_TOKEN",
    "OAUTH2_EXPIRY_DATE",
    "GITLAB_CLIENT_ID",
    "CLIENT_ID",
];

/// A selected account ignores tokens inherited from a terminal/other profile. No process
/// environment is changed: this affects this one command only.
pub(crate) fn program(tool: &str) -> PathBuf {
    #[cfg(test)]
    if let Some(path) = TEST_PROGRAMS.with(|p| p.borrow().get(tool).cloned()) {
        return path;
    }
    PathBuf::from(tool)
}

pub(crate) fn account_command(tool: &str, account: &Account) -> Result<Command, String> {
    account.validate()?;
    let mut cmd = Command::new(program(tool));
    crate::shell_path::apply(&mut cmd);
    cmd.current_dir(std::env::temp_dir())
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_PROMPT", "1")
        .env("NO_COLOR", "1")
        .env_remove("GH_HOST")
        .env_remove("GH_REPO")
        .env_remove("GITLAB_HOST")
        .env_remove("GITLAB_REPO")
        .env_remove("GL_HOST")
        .env_remove("GITLAB_URI")
        .env_remove("GITLAB_HEAD_REPO")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("GH_FORCE_TTY")
        .env_remove("GLAB_IS_OAUTH2")
        .env_remove("GH_DEBUG")
        .env_remove("GLAB_DEBUG")
        .env_remove("GLAB_DEBUG_HTTP");
    for name in if tool == "gh" { GH_TOKENS } else { GL_TOKENS } {
        cmd.env_remove(name);
    }
    if tool == "glab" {
        // An inherited CI session or API-host override must not reroute a selected token.
        cmd.env("GLAB_ENABLE_CI_AUTOLOGIN", "false")
            .env("GITLAB_API_HOST", &account.host)
            .env("GLAB_API_PROTOCOL", "https")
            .env_remove("API_PROTOCOL")
            .env_remove("GITLAB_SUBFOLDER")
            .env_remove("GLAB_USER")
            .env_remove("GLAB_USE_KEYRING")
            .env_remove("USE_KEYRING");
    }
    if !account.config_dir.is_empty() {
        cmd.env(
            if tool == "gh" {
                "GH_CONFIG_DIR"
            } else {
                "GLAB_CONFIG_DIR"
            },
            &account.config_dir,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    Ok(cmd)
}

/// Output is never used as an error message: a CLI/keyring error might echo a secret.
pub(super) fn read_private(cmd: Command) -> Result<String, String> {
    let out = crate::processes::run(
        cmd,
        crate::processes::Options {
            timeout: Some(Duration::from_secs(30)),
            output_limit: 65536,
            stderr_limit: Some(65536),
            ..Default::default()
        },
    )
    .and_then(|out| out.complete())
    .map_err(|_| "The account CLI could not complete its request".to_owned())?;
    if !out.status.success() {
        return Err(
            "The selected account is unavailable; check its CLI sign-in and version".into(),
        );
    }
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| "Invalid account CLI response".into())
}

pub(super) fn install_token(cmd: &mut Command, tool: &str, account: &Account, token: &Secret) {
    let name = if tool == "glab" {
        "GITLAB_TOKEN"
    } else if account.host.eq_ignore_ascii_case("github.com") {
        "GH_TOKEN"
    } else {
        "GH_ENTERPRISE_TOKEN"
    };
    cmd.env(name, &token.value);
    if tool == "glab" {
        cmd.env("GLAB_IS_OAUTH2", if token.oauth { "true" } else { "false" });
        if token.oauth {
            // Pin the verified access token for this command. A later config/account edit
            // must not let glab refresh into a different identity during a mutation.
            // Keep OAuth's Bearer scheme, but disable automatic renewal with a nonsecret
            // sentinel and nonexpiring process-local metadata (never written to config).
            cmd.env("OAUTH2_REFRESH_TOKEN", "gitcraft-immutable-access-token")
                .env("OAUTH2_EXPIRY_DATE", "9999-12-31T23:59:59Z");
        }
    }
}

/// gh supports selecting a stored username directly. glab stores one token per host in a
/// config folder: resolve that token once, then verify its owner before using it for a write.
pub(super) fn token(tool: &str, account: &Account) -> Result<Secret, String> {
    if !account.selected() {
        return Err("Choose an account username first".to_owned());
    }
    let mut cmd = account_command(tool, account)?;
    match tool {
        "gh" => {
            cmd.args([
                "auth",
                "token",
                "--hostname",
                &account.host,
                "--user",
                &account.user,
            ]);
        }
        "glab" => {
            cmd.args([
                "config",
                "get",
                "token",
                "--host",
                &account.host,
                "--global",
            ]);
        }
        _ => return Err("Unsupported account CLI".to_owned()),
    }
    let token = read_private(cmd)?;
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("No usable token for the selected account; sign in with its CLI".to_owned());
    }
    let mut secret = Secret {
        value: token,
        oauth: false,
    };
    if tool == "glab" {
        let mut config = account_command(tool, account)?;
        config.args([
            "config",
            "get",
            "is_oauth2",
            "--host",
            &account.host,
            "--global",
        ]);
        secret.oauth = read_private(config)?.eq_ignore_ascii_case("true");
        if secret.oauth {
            // A known different login must never be renewed just to discover its owner.
            // `user` is nonsecret CLI login metadata; the API still verifies the token.
            let mut owner = account_command(tool, account)?;
            owner.args(["config", "get", "user", "--host", &account.host, "--global"]);
            if !read_private(owner)?.eq_ignore_ascii_case(&account.user) {
                return Err("The selected GitLab configuration belongs to another user; sign in as the selected account in its account folder".to_owned());
            }
            // Refresh only through the deliberately selected CLI configuration, with no
            // inherited access/refresh credentials. Its owner must match before proceeding.
            let mut refresh = account_command(tool, account)?;
            refresh.args(["api", "user", "--hostname", &account.host]);
            verify_user_output(tool, account, &read_private(refresh)?)?;
            let mut current = account_command(tool, account)?;
            current.args([
                "config",
                "get",
                "token",
                "--host",
                &account.host,
                "--global",
            ]);
            secret.value = read_private(current)?;
            if secret.value.is_empty() || secret.value.chars().any(char::is_whitespace) {
                return Err("No usable token after refreshing the selected account".to_owned());
            }
        }
        // Verify the exact immutable token passed to CLI writes and HTTPS Git, including
        // a replacement persisted during refresh or a concurrent CLI account change.
        verify_user(tool, account, &secret)?;
    }
    Ok(secret)
}

pub(super) fn verify_user(tool: &str, account: &Account, token: &Secret) -> Result<String, String> {
    let mut cmd = account_command(tool, account)?;
    install_token(&mut cmd, tool, account, token);
    cmd.args(["api", "user", "--hostname", &account.host]);
    verify_user_output(tool, account, &read_private(cmd)?)
}

pub(super) fn verify_user_output(
    tool: &str,
    account: &Account,
    json: &str,
) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "The account CLI did not return an account name")?;
    let name = value
        .get(if tool == "gh" { "login" } else { "username" })
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if !name.eq_ignore_ascii_case(&account.user) {
        return Err(format!(
            "The selected {tool} configuration is signed in as a different user; sign in as {} in its account folder",
            account.user
        ));
    }
    Ok(format!("{} as {name}", account.host))
}

pub(crate) fn check_account(tool: &str, account: &Account) -> Result<String, String> {
    if tool == "tea" {
        return crate::forge::selected_tea_metadata(&account.user)
            .map(|(host, user)| format!("{host} as {user} ({})", account.user))
            .ok_or_else(|| "The selected tea HTTPS login was not found".to_owned());
    }
    let token = token(tool, account)?;
    verify_user(tool, account, &token)
}

pub(crate) fn configure_cli(cmd: &mut Command, tool: &str, host: &str) -> Result<(), String> {
    let context = current();
    if let Some(error) = &context.error {
        return Err(error.clone());
    }
    let account = if tool == "gh" {
        &context.choices.github
    } else {
        &context.choices.gitlab
    };
    account.validate()?;
    if !account.selected() {
        return Ok(());
    }
    if !account.host.eq_ignore_ascii_case(host) {
        return Err(format!(
            "This profile selects {tool} account {} on {}; it cannot be used on {host}",
            account.user, account.host
        ));
    }
    let configured = account_command(tool, account)?;
    for (name, value) in configured.get_envs() {
        if let Some(value) = value {
            cmd.env(name, value);
        } else {
            cmd.env_remove(name);
        }
    }
    let token = token(tool, account)?;
    install_token(cmd, tool, account, &token);
    Ok(())
}

/// The self helper is called only by Git. Reject a request for any other protocol/server,
/// and do nothing for store/erase: selected-account secrets are never persisted by gitcraft.
pub(crate) fn credential_reply(
    tool: &str,
    account: &Account,
    operation: &str,
    input: &str,
) -> Result<Option<String>, String> {
    if operation != "get" {
        return Ok(None);
    }
    if tool == "tea" {
        Account {
            user: "tea".to_owned(),
            ..account.clone()
        }
        .validate()?;
    } else {
        account.validate()?;
    }
    let get = |key: &str| {
        input
            .lines()
            .find_map(|line| line.strip_prefix(key).and_then(|s| s.strip_prefix('=')))
    };
    if get("protocol") != Some("https")
        || !get("host").is_some_and(|host| host.eq_ignore_ascii_case(&account.host))
    {
        return Ok(None);
    }
    let (user, token) = if tool == "tea" {
        crate::forge::selected_tea_secret(&account.user, &account.host)?
    } else {
        (account.user.clone(), token(tool, account)?.value)
    };
    Ok(Some(format!("username={user}\npassword={token}\n\n")))
}

pub fn credential_helper() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--gitcraft-profile-credential") {
        return None;
    }
    let run = || -> Result<(), String> {
        let [_, tool, encoded, operation] = args.as_slice() else {
            return Err("Invalid profile credential helper request".to_owned());
        };
        if !matches!(tool.as_str(), "gh" | "glab" | "tea") {
            return Err("Unsupported profile credential helper".to_owned());
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| "Invalid account selection")?;
        let account: Account =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid account selection")?;
        let mut input = String::new();
        std::io::stdin()
            .take(65536)
            .read_to_string(&mut input)
            .map_err(|_| "Could not read Git's credential request")?;
        if let Some(reply) = credential_reply(tool, &account, operation, &input)? {
            std::io::stdout()
                .write_all(reply.as_bytes())
                .map_err(|_| "Could not answer Git's credential request")?;
        }
        Ok(())
    };
    Some(match run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("gitcraft profile credentials: {error}");
            1
        }
    })
}
