//! Local public-key registration with a captured, explicitly named profile account.
//! No global CLI account switch; errors never contain raw tool/HTTP diagnostics.
use super::{Forge, Hosting, http::Api};
use crate::connections::{self, Context};
use serde_json::{Value, json};
pub(crate) mod public_key;
use public_key::PublicKey;
use std::process::Command;
use std::time::Duration;

const FAILED: &str = "Public-key registration failed. Check the selected account's key-management permission and installed tool/login; no private key or tool diagnostics are displayed.";

pub(crate) struct SshKeyAccount {
    owner: String,
    target: Target,
}
enum Target {
    Github { host: String, command: Command },
    Gitea(Box<Api>),
}
impl SshKeyAccount {
    pub fn owner(&self) -> &str {
        &self.owner
    }
    pub fn upload(
        self,
        title: &str,
        public_key: &PublicKey,
        active: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if !active() {
            return Err("Public-key upload cancelled before sending.".into());
        }
        if title.len() > 512 || title.chars().any(char::is_control) {
            return Err(
                "Choose a public key title within 512 bytes, without control characters.".into(),
            );
        }
        let body = json!({"title":title,"key":public_key.as_str()});
        match self.target {
            Target::Github { host, mut command } => {
                command.args([
                    "api",
                    "--hostname",
                    &host,
                    "--method",
                    "POST",
                    "user/keys",
                    "--input",
                    "-",
                ]);
                execute(command, Some(&body), active)?;
            }
            Target::Gitea(api) => {
                api.send("POST", "user/keys", Some(&body))
                    .map_err(|_| FAILED.to_owned())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn prepare(
    hosting: &Hosting,
    account: &str,
    active: &dyn Fn() -> bool,
) -> Result<SshKeyAccount, String> {
    if account.trim().is_empty() {
        return Err(
            "Choose the existing GitHub username or tea login alias before checking the account."
                .into(),
        );
    }
    if !hosting.web.starts_with("https://") || hosting.web.contains(['\n', '\r']) {
        return Err("SSH public-key registration requires a known HTTPS forge server.".into());
    }
    let mut context = connections::current();
    match hosting.forge {
        Forge::GitHub => {
            context.choices.github.host = hosting.host.clone();
            context.choices.github.user = account.into();
            context.choices.github.validate()?;
            let command = github_command(&context, &hosting.host)?;
            let mut check = duplicate_command(&command);
            check.args(["api", "--hostname", &hosting.host, "user"]);
            let answer = execute(check, None, active)?;
            let owner = answer
                .get("login")
                .and_then(Value::as_str)
                .filter(|owner| owner.eq_ignore_ascii_case(account))
                .ok_or("The GitHub account did not match the selected username.")?
                .to_owned();
            Ok(SshKeyAccount {
                owner,
                target: Target::Github {
                    host: hosting.host.clone(),
                    command,
                },
            })
        }
        Forge::Forgejo | Forge::Gitea => {
            context.choices.tea_login = account.into();
            context.choices.tea_host = hosting
                .web
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .into();
            context.choices.validate()?;
            let api = context.run(|| super::gitea::ssh_key_api(hosting));
            if !api.logged_in() {
                return Err("No existing tea login for this HTTPS server was found.".into());
            }
            if !active() {
                return Err("Account check cancelled.".into());
            }
            let answer = api.get("user").map_err(|_| FAILED.to_owned())?;
            let owner = answer
                .get("login")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                .ok_or("The forge did not identify its current account.")?
                .to_owned();
            Ok(SshKeyAccount {
                owner,
                target: Target::Gitea(Box::new(api)),
            })
        }
        Forge::GitLab => Err("This SSH helper registers keys on GitHub, Forgejo and Gitea.".into()),
    }
}

/// Capture the exact token once. The owner probe and eventual mutation use copies of
/// the same command environment, rather than resolving a possibly changed login again.
fn github_command(context: &Context, host: &str) -> Result<Command, String> {
    let mut command = Command::new(connections::program("gh"));
    crate::shell_path::apply(&mut command);
    context.run(|| connections::configure_cli(&mut command, "gh", host))?;
    command
        .current_dir(std::env::temp_dir())
        .env_remove("GH_DEBUG")
        .env_remove("GH_FORCE_TTY")
        .env_remove("GH_REPO")
        .env("GH_PROMPT_DISABLED", "1");
    Ok(command)
}
fn duplicate_command(command: &Command) -> Command {
    let mut copy = Command::new(command.get_program());
    if let Some(cwd) = command.get_current_dir() {
        copy.current_dir(cwd);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            copy.env(key, value);
        } else {
            copy.env_remove(key);
        }
    }
    copy
}
fn execute(
    command: Command,
    body: Option<&Value>,
    active: &dyn Fn() -> bool,
) -> Result<Value, String> {
    if !active() {
        return Err("Account action cancelled.".into());
    }
    let output = crate::bounded_process::run_input(
        command,
        body.map(|value| value.to_string().into_bytes()),
        active,
        Duration::from_secs(20),
        32768,
        16384,
    )
    .map_err(|_| FAILED.to_owned())?;
    if !output.status.success() {
        return Err(FAILED.into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| FAILED.to_owned())
}
