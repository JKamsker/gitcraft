//! Profile connection choices. Never switch a CLI's global active account or store a token.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(test)]
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Account {
    pub host: String,
    pub user: String,
    /// Optional independent CLI configuration folder, e.g. two glab accounts on one host.
    pub config_dir: String,
}

impl Account {
    pub fn on(host: &str) -> Self {
        Self {
            host: host.to_owned(),
            ..Self::default()
        }
    }
    pub fn selected(&self) -> bool {
        !self.user.trim().is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.selected() {
            return if self.config_dir.trim().is_empty() {
                Ok(())
            } else {
                Err("Choose the account's username as well as its configuration folder".to_owned())
            };
        }
        if self.host.is_empty()
            || self.host.starts_with(['-', '.'])
            || !self
                .host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
        {
            return Err("An account needs a plain server hostname (and optional port)".to_owned());
        }
        if self.user.starts_with('-')
            || !self
                .user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("Choose a plain account username".to_owned());
        }
        if !self.config_dir.trim().is_empty() && !Path::new(&self.config_dir).is_absolute() {
            return Err("The account configuration folder must be an absolute path".to_owned());
        }
        if self.config_dir.chars().any(char::is_control) {
            return Err("The account folder cannot contain control characters".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Choices {
    pub ssh_key: String,
    pub github: Account,
    pub gitlab: Account,
    pub tea_login: String,
    /// HTTPS host for the selected tea alias; metadata only, never its token.
    pub tea_host: String,
}

impl Default for Choices {
    fn default() -> Self {
        Self {
            ssh_key: String::new(),
            github: Account::on("github.com"),
            gitlab: Account::on("gitlab.com"),
            tea_login: String::new(),
            tea_host: String::new(),
        }
    }
}

impl Choices {
    pub fn validate(&self) -> Result<(), String> {
        self.github.validate()?;
        self.gitlab.validate()?;
        if !self.tea_login.is_empty() {
            Account {
                host: self.tea_host.clone(),
                user: "tea".to_owned(),
                config_dir: String::new(),
            }
            .validate()
            .map_err(|_| {
                "Set the tea login's HTTPS server, or use Check account to find it".to_owned()
            })?;
        }
        if self.ssh_key.chars().any(char::is_control)
            || self.tea_login.chars().any(char::is_control)
        {
            return Err(
                "SSH key paths and tea login names cannot contain control characters".to_owned(),
            );
        }
        if !self.ssh_key.is_empty() && !Path::new(&self.ssh_key).is_absolute() {
            return Err("Choose an absolute path to the SSH private key".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Context {
    pub git_config: Option<PathBuf>,
    pub lease: Option<Arc<ConfigLease>>,
    pub choices: Choices,
    pub error: Option<String>,
}

/// A worker holds its immutable config alive across profile edits/deletion/id reuse.
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ConfigLease {
    pub path: PathBuf,
}
fn leases() -> &'static Mutex<std::collections::HashMap<PathBuf, Weak<ConfigLease>>> {
    static LEASES: std::sync::OnceLock<
        Mutex<std::collections::HashMap<PathBuf, Weak<ConfigLease>>>,
    > = std::sync::OnceLock::new();
    LEASES.get_or_init(Default::default)
}
impl ConfigLease {
    pub fn acquire(path: PathBuf) -> Arc<Self> {
        let mut known = leases()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(lease) = known.get(&path).and_then(Weak::upgrade) {
            return lease;
        }
        let lease = Arc::new(Self { path: path.clone() });
        known.insert(path, Arc::downgrade(&lease));
        lease
    }
}
impl Drop for ConfigLease {
    fn drop(&mut self) {
        let mut known = leases()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if known
            .get(&self.path)
            .is_some_and(|weak| std::ptr::eq(weak.as_ptr(), self))
        {
            known.remove(&self.path);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

static ACTIVE: RwLock<Option<Context>> = RwLock::new(None);
thread_local! { static PINNED: RefCell<Option<Context>> = const { RefCell::new(None) }; }

pub(crate) fn set(context: Context) {
    *ACTIVE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(context);
}

pub(crate) fn current() -> Context {
    PINNED
        .with(|value| value.borrow().clone())
        .unwrap_or_else(|| {
            ACTIVE
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
                .unwrap_or_default()
        })
}

impl Context {
    /// Nested jobs keep the initiating profile even if the UI switches meanwhile.
    pub fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        struct Restore(Option<Context>);
        impl Drop for Restore {
            fn drop(&mut self) {
                PINNED.with(|p| *p.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(PINNED.with(|p| p.replace(Some(self.clone()))));
        job()
    }
}

pub(crate) fn spawn<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    let context = current();
    std::thread::spawn(move || context.run(job))
}

/// Git executes core.sshCommand with its shell, including on Git for Windows.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn sign_in_command(tool: &str, account: &Account) -> String {
    let variable = if tool == "gh" {
        "GH_CONFIG_DIR"
    } else {
        "GLAB_CONFIG_DIR"
    };
    #[cfg(windows)]
    {
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        format!(
            "$env:{variable}={}; {tool} auth login --hostname {}",
            quote(&account.config_dir),
            quote(&account.host)
        )
    }
    #[cfg(not(windows))]
    {
        format!(
            "{variable}={} {tool} auth login --hostname {}",
            shell_quote(&account.config_dir),
            shell_quote(&account.host)
        )
    }
}

/// Metadata only: the token is resolved by this process when Git actually asks for a secret.
pub(crate) fn helper_command(tool: &str, account: &Account, executable: &Path) -> String {
    #[cfg(windows)]
    let executable = PathBuf::from(executable.to_string_lossy().replace('\\', "/"));
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(account).unwrap_or_default());
    format!(
        "!{} --gitcraft-profile-credential {tool} {encoded}",
        shell_quote(&executable.to_string_lossy())
    )
}

#[cfg(test)]
thread_local! { pub(crate) static TEST_PROGRAMS: RefCell<std::collections::HashMap<String,PathBuf>> = RefCell::new(std::collections::HashMap::new()); }

struct Secret {
    value: String,
    oauth: bool,
}

const GH_TOKENS: &[&str] = &[
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];
const GL_TOKENS: &[&str] = &[
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
fn read_private(cmd: Command) -> Result<String, String> {
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

fn install_token(cmd: &mut Command, tool: &str, account: &Account, token: &Secret) {
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
fn token(tool: &str, account: &Account) -> Result<Secret, String> {
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

fn verify_user(tool: &str, account: &Account, token: &Secret) -> Result<String, String> {
    let mut cmd = account_command(tool, account)?;
    install_token(&mut cmd, tool, account, token);
    cmd.args(["api", "user", "--hostname", &account.host]);
    verify_user_output(tool, account, &read_private(cmd)?)
}

fn verify_user_output(tool: &str, account: &Account, json: &str) -> Result<String, String> {
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

#[cfg(test)]
mod tests;

/// SSH command-line identities are additive with ~/.ssh/config's IdentityFile entries.
/// Refuse another effective identity rather than silently pushing as another account.
pub(crate) fn validate_ssh_identities(
    key: &str,
    config: &str,
    ssh_args: &[String],
) -> Result<(), String> {
    let mut seen = false;
    let normalized = |value: &str| {
        let path = PathBuf::from(value);
        let path = path.canonicalize().unwrap_or(path);
        #[cfg(windows)]
        let text = path.to_string_lossy().replace('\\', "/").to_lowercase();
        #[cfg(not(windows))]
        let text = path.to_string_lossy().into_owned();
        text
    };
    let selected = normalized(key);
    for identity in config
        .split_terminator('\n')
        .filter_map(|line| line.strip_prefix("identityfile "))
    {
        if identity.chars().any(char::is_control) {
            return Err(
                "SSH returned an ambiguous identity path containing control characters".into(),
            );
        }
        if identity == "none" {
            continue;
        }
        let expanded;
        let identity =
            if identity.starts_with('~') || identity.contains('%') || identity.contains("${") {
                // `ssh -G` leaves IdentityFile unexpanded. ControlPath uses OpenSSH's same
                // tilde/environment/percent expansion, including passwd home and %C, and -G
                // prints that expanded value without opening a connection or reading a key.
                let mut cmd = ssh_command(key);
                // Fence the complete raw expansion: a Unix filename can contain LF/CR.
                // Taking only its first line would accept a different key's selected prefix.
                let fence = "__GITCRAFT_IDENTITY_END__";
                let tagged = format!("{identity}{fence}");
                let quoted = tagged.replace('\\', "\\\\").replace('"', "\\\"");
                cmd.args(["-G", "-o", &format!("ControlPath=\"{quoted}\"")])
                    .args(ssh_args);
                let output = read_private(cmd)?;
                expanded = expanded_identity(&output, fence)?;
                expanded.as_str()
            } else {
                identity
            };
        if normalized(identity) != selected {
            return Err("SSH configuration offers another IdentityFile for this host. Keep only the profile's selected identity for this host before pushing.".to_owned());
        }
        seen = true;
    }
    if !seen {
        return Err("SSH could not select this profile's identity file".to_owned());
    }
    Ok(())
}

/// Read one fenced value without line splitting, CR removal, or whitespace trimming.
/// `rfind` includes any embedded fence in the path; a later field containing a fence
/// makes the result fail closed because the intervening newlines remain in the value.
fn expanded_identity(output: &str, fence: &str) -> Result<String, String> {
    let mut fields = output
        .match_indices("controlpath ")
        .filter(|(at, _)| *at == 0 || output.as_bytes().get(at - 1) == Some(&b'\n'));
    let start = fields
        .next()
        .ok_or("SSH did not expand the configured identity path")?
        .0
        + "controlpath ".len();
    if fields.next().is_some() {
        return Err("SSH returned ambiguous identity expansion fields".into());
    }
    let end = output
        .rfind(fence)
        .filter(|end| *end >= start)
        .ok_or("SSH did not finish expanding the configured identity path")?;
    let tail = &output[end + fence.len()..];
    if !tail.starts_with('\n') && !tail.starts_with("\r\n") {
        return Err("SSH returned an ambiguous identity expansion boundary".into());
    }
    let value = &output[start..end];
    if value.chars().any(char::is_control) {
        return Err("SSH returned an identity path containing control characters".into());
    }
    Ok(value.to_owned())
}

pub(crate) fn ssh_helper_command(key: &str, executable: &Path) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(key.as_bytes());
    #[cfg(windows)]
    let executable = PathBuf::from(executable.to_string_lossy().replace('\\', "/"));
    format!(
        "{} --gitcraft-profile-ssh {encoded}",
        shell_quote(&executable.to_string_lossy())
    )
}

fn ssh_command(key: &str) -> Command {
    let mut cmd = Command::new("ssh");
    crate::shell_path::apply(&mut cmd);
    cmd.args(["-o", "IdentitiesOnly=yes", "-i", key]);
    cmd
}

pub fn ssh_helper() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--gitcraft-profile-ssh") {
        return None;
    }
    let run = || -> Result<i32, String> {
        let [_, encoded, rest @ ..] = args.as_slice() else {
            return Err("Invalid profile SSH request".to_owned());
        };
        let key = String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| "Invalid profile SSH key path")?,
        )
        .map_err(|_| "Invalid profile SSH key path")?;
        run_ssh(&key, rest)
    };
    Some(match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("gitcraft profile SSH: {error}");
            1
        }
    })
}

/// Git's Plink dialect is not OpenSSH: notably -P is a port in Plink and a tag in
/// modern OpenSSH. Validate before even the -G probe, so an explicit local variant
/// cannot silently change the endpoint. Operand values and the remote command are data.
fn validate_ssh_options(args: &[String]) -> Result<(), String> {
    const BAD: &str = "This profile's key uses OpenSSH, but Git supplied incompatible SSH options. Set this repository's ssh.variant to auto or ssh, or use its own matching SSH command.";
    let mut at = 0;
    let mut have_host = false;
    while let Some(arg) = args.get(at) {
        if arg == "--" {
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            // OpenSSH parses options again after its destination, until the remote
            // command (or --) starts. Plink options there are just as ambiguous.
            if have_host {
                break;
            }
            have_host = true;
            at += 1;
            continue;
        }
        if arg == "-batch" {
            return Err(BAD.into());
        }
        let mut chars = arg[1..].chars().peekable();
        while let Some(option) = chars.next() {
            if option == 'P' {
                return Err(BAD.into());
            }
            if "1246AaCfGgKkMNnqsTtVvXxYy".contains(option) {
                continue;
            }
            if "BbcDEeFIiJLlmOopQRSWw".contains(option) {
                if chars.peek().is_none() {
                    at += 1;
                    if args.get(at).is_none() {
                        return Err(BAD.into());
                    }
                }
                break;
            }
            return Err(BAD.into());
        }
        at += 1;
    }
    Ok(())
}

/// Shared with the helper-boundary fixtures: these are the exact checks and transport
/// launch used by --gitcraft-profile-ssh, without reading any key contents in gitcraft.
fn run_ssh(key: &str, rest: &[String]) -> Result<i32, String> {
    if !Path::new(key).is_absolute() || key.chars().any(char::is_control) {
        return Err("Invalid profile SSH key path".to_owned());
    }
    validate_ssh_options(rest)?;
    let mut inspect = ssh_command(key);
    inspect.arg("-G").args(rest);
    let config = read_private(inspect)?;
    validate_ssh_identities(key, &config, rest)?;
    let status = ssh_command(key)
        .args(rest)
        .status()
        .map_err(|_| "Could not start SSH")?;
    Ok(status.code().unwrap_or(1))
}

/// Nonsecret metadata for push-preflight diagnostics. A transport username such as SSH's
/// `git` is not this account, and a tea login alias is not necessarily its user's name.
#[allow(dead_code)] // Consumed by the independently developed push-preflight feature at integration.
pub(crate) fn expected_account(host: &str) -> Option<String> {
    let context = current();
    [&context.choices.github, &context.choices.gitlab]
        .into_iter()
        .find(|account| account.selected() && account.host.eq_ignore_ascii_case(host))
        .map(|account| account.user.clone())
}
