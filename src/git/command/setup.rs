//! Import clones use only the previewed URL and captured explicit authentication.
//! No global/profile include, parent repository, .netrc, URL rewrite or ambient SSH
//! identity is part of this temporary Git context. Nothing here is exported.
use super::practice_command;
use crate::connections::{self, Choices};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir()
            .canonicalize()
            .map_err(|e| e.to_string())?;
        for _ in 0..100 {
            let path = root.join(format!(
                "gitcraft-setup-auth-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut mkdir = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                mkdir.mode(0o700);
            }
            match mkdir.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(format!("Could not create an isolated setup context: {e}")),
            }
        }
        Err("Could not create a new isolated setup context".into())
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// Git itself has an empty HOME, so curl cannot pick up a personal .netrc. Only a
// selected fixed helper receives these original local configuration-folder paths.
struct HelperPaths(Vec<(&'static str, Option<String>)>);
impl HelperPaths {
    fn capture() -> Result<Self, String> {
        let mut paths = Vec::new();
        for name in [
            "HOME",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "XDG_CONFIG_HOME",
            "APPDATA",
            "LOCALAPPDATA",
            "GH_CONFIG_DIR",
            "GLAB_CONFIG_DIR",
        ] {
            let value = std::env::var_os(name)
                .map(|v| {
                    v.into_string().map_err(|_| {
                        format!("{name} must be UTF-8 for selected setup authentication")
                    })
                })
                .transpose()?;
            if value
                .as_ref()
                .is_some_and(|v| v.len() > 32768 || v.chars().any(char::is_control))
            {
                return Err(format!(
                    "{name} contains unsupported characters for selected setup authentication"
                ));
            }
            paths.push((name, value));
        }
        Ok(Self(paths))
    }
    fn wrap(&self, command: &str) -> String {
        let mut script = String::from("f() { ");
        for (name, value) in &self.0 {
            match value {
                Some(value) => script.push_str(&format!(
                    "{name}={}; export {name}; ",
                    connections::shell_quote(value)
                )),
                None => script.push_str(&format!("unset {name}; ")),
            }
        }
        // Git appends the credential operation/SSH operands to f. Preserve them as data.
        script.push_str(&format!("exec {command} \"$@\"; }}; f"));
        script
    }
}
struct CloneCommand {
    command: Command,
    workspace: Workspace,
}

fn clone_command(
    parent: &Path,
    source: &str,
    destination: &Path,
    remote: &str,
    local: bool,
    executable: &Path,
) -> Result<CloneCommand, String> {
    if crate::remote::is_remote(parent) || !parent.is_absolute() || !destination.is_absolute() {
        return Err("Setup clones require local absolute folders".into());
    }
    let choices = if local {
        Choices::default()
    } else {
        let context = connections::current();
        if let Some(error) = context.error {
            return Err(error);
        }
        context.choices.validate()?;
        let (sanitized, changed) = crate::portability::sanitize_url(source)?;
        if changed || sanitized != source {
            return Err("Preview a credential-free HTTPS or SSH source before cloning".into());
        }
        if !source.starts_with("https://") && context.choices.ssh_key.is_empty() {
            return Err("Select an explicit SSH key in this profile before restoring an SSH repository; ambient identities are not used".into());
        }
        if !source.starts_with("https://") && !Path::new(&context.choices.ssh_key).is_file() {
            return Err("The explicitly selected SSH key file is unavailable; finish this profile's local setup before restoring SSH repositories".into());
        }
        context.choices
    };
    let workspace = Workspace::new()?;
    let mut command = practice_command(&workspace.0);
    command
        .env("GIT_CEILING_DIRECTORIES", &workspace.0)
        .env("HOME", &workspace.0)
        .env("USERPROFILE", &workspace.0)
        .env("XDG_CONFIG_HOME", &workspace.0)
        .env_remove("HOMEDRIVE")
        .env_remove("HOMEPATH")
        .env_remove("NETRC")
        .env_remove("GIT_SSL_NO_VERIFY")
        .env_remove("GIT_SSL_CERT")
        .env_remove("GIT_SSL_KEY")
        .env_remove("GIT_SSL_CERT_PASSWORD_PROTECTED")
        .env_remove("GIT_SSH_VARIANT")
        .env_remove("GIT_PROXY_COMMAND")
        .env_remove("GIT_ATTR_SOURCE")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env(
            "GIT_ALLOW_PROTOCOL",
            if local {
                "file"
            } else if source.starts_with("https://") {
                "https"
            } else {
                "ssh"
            },
        );
    let mut config = vec![
        ("credential.helper".to_owned(), String::new()),
        ("credential.interactive".into(), "false".into()),
        ("http.followRedirects".into(), "false".into()),
        ("http.extraHeader".into(), String::new()),
    ];
    if !local && source.starts_with("https://") {
        let host = source
            .strip_prefix("https://")
            .and_then(|s| s.split_once('/'))
            .map(|(h, _)| h)
            .ok_or("Invalid previewed HTTPS host")?;
        let mut selected = Vec::new();
        for (tool, account) in [("gh", &choices.github), ("glab", &choices.gitlab)] {
            if account.selected() && account.host.eq_ignore_ascii_case(host) {
                selected.push((tool, account.clone()));
            }
        }
        if !choices.tea_login.is_empty() && choices.tea_host.eq_ignore_ascii_case(host) {
            selected.push((
                "tea",
                connections::Account {
                    host: choices.tea_host.clone(),
                    user: choices.tea_login.clone(),
                    config_dir: String::new(),
                },
            ));
        }
        if selected.len() > 1 {
            return Err("Select one profile account for the previewed server before restoring this repository".into());
        }
        if let Some((tool, account)) = selected.pop() {
            let helpers = HelperPaths::capture()?;
            let generated = connections::helper_command(tool, &account, executable);
            let raw = generated
                .strip_prefix('!')
                .ok_or("Could not prepare the selected authentication helper")?;
            config.push((format!("credential.https://{host}.helper"), String::new()));
            config.push((
                format!("credential.https://{host}.helper"),
                format!("!{}", helpers.wrap(raw)),
            ));
        }
    } else if !local {
        let helpers = HelperPaths::capture()?;
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let raw = format!(
            "{} -F {} -o BatchMode=yes",
            connections::ssh_helper_command(&choices.ssh_key, executable),
            connections::shell_quote(null)
        );
        config.push(("core.sshCommand".into(), helpers.wrap(&raw)));
        config.push(("ssh.variant".into(), "ssh".into()));
    }
    command.env("GIT_CONFIG_COUNT", config.len().to_string());
    for (index, (key, value)) in config.into_iter().enumerate() {
        command
            .env(format!("GIT_CONFIG_KEY_{index}"), key)
            .env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    command
        .args([
            "--no-lazy-fetch",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "init.templateDir=",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            if local {
                "protocol.file.allow=always"
            } else {
                "protocol.file.allow=never"
            },
            "clone",
            "--no-checkout",
            "--template=",
            "--origin",
            remote,
            "--",
            source,
        ])
        .arg(destination);
    Ok(CloneCommand { command, workspace })
}

pub(crate) fn setup_clone(
    parent: &Path,
    source: &str,
    destination: &Path,
    remote: &str,
    local: bool,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|_| "Could not locate the selected profile authentication helper")?;
    let CloneCommand {
        command,
        workspace: _workspace,
    } = clone_command(parent, source, destination, remote, local, &executable)?;
    let out = crate::bounded_process::run(command, active, Duration::from_secs(1800))?;
    if out.status.success() {
        Ok(())
    } else {
        Err("The previewed source could not be cloned with this profile's explicit authentication. Check local setup and use a new destination; ambient credentials and URL rewrites are not used".into())
    }
}

#[cfg(test)]
mod tests;
