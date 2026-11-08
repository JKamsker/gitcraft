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
            let mkdir = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let mkdir = {
                use std::os::unix::fs::DirBuilderExt;
                let mut mkdir = mkdir;
                mkdir.mode(0o700);
                mkdir
            };
            match mkdir.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => {
                    return Err(crate::i18n::message(
                        "backend-could-not-create-an-isolated-setup-context-e-31868f",
                        &[("detail", (e).to_string().into())],
                    ));
                }
            }
        }
        Err(
            crate::i18n::text("backend-could-not-create-a-new-isolated-setup-context-b2354d")
                .into(),
        )
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
                        crate::i18n::message(
                            "backend-name-must-be-utf-8-for-selected-setup-authentication-e0b537",
                            &[("name", (name).to_string().into())],
                        )
                    })
                })
                .transpose()?;
            if value
                .as_ref()
                .is_some_and(|v| v.len() > 32768 || v.chars().any(char::is_control))
            {
                return Err(crate::i18n::message(
                    "backend-name-contains-unsupported-characters-for-selected-setup-authentic-aabb5c",
                    &[("name", (name).to_string().into())],
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

fn transport_command(source: &str, local: bool, executable: &Path) -> Result<CloneCommand, String> {
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
            return Err(crate::i18n::text(
                "backend-preview-a-credential-free-https-or-ssh-source-before-cloning-399db2",
            )
            .into());
        }
        if !source.starts_with("https://") && context.choices.ssh_key.is_empty() {
            return Err(crate::i18n::text(
                "backend-select-an-explicit-ssh-key-in-this-profile-before-restoring-an-ss-9bee64",
            )
            .into());
        }
        if !source.starts_with("https://") && !Path::new(&context.choices.ssh_key).is_file() {
            return Err(crate::i18n::text(
                "backend-the-explicitly-selected-ssh-key-file-is-unavailable-finish-this-p-bde415",
            )
            .into());
        }
        context.choices
    };
    if !local && let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
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
            .ok_or(crate::i18n::text(
                "backend-invalid-previewed-https-host-227969",
            ))?;
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
        // Match the reviewed profile config: each explicit same-host choice resets
        // the previous helper (GitHub, then GitLab, then Tea). Never fall back to an
        // ambient helper when the final explicitly selected account refuses auth.
        if let Some((tool, account)) = selected.pop() {
            let helpers = HelperPaths::capture()?;
            let generated = connections::helper_command(tool, &account, executable);
            let raw = generated.strip_prefix('!').ok_or(crate::i18n::text(
                "backend-could-not-prepare-the-selected-authentication-helper-826285",
            ))?;
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
    command.args([
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
    ]);
    Ok(CloneCommand { command, workspace })
}

/// Keep canonical filesystem identities in receipts, but pass Git its native path
/// spelling. Git for Windows treats `\\?\C:\...` as an invalid path (and can
/// mistake the embedded colon for an SSH transport); UNC paths need `//server/...`.
pub(crate) fn setup_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix("//?/UNC/") {
            format!("//{rest}")
        } else {
            text.strip_prefix("//?/").unwrap_or(&text).to_owned()
        }
    }
    #[cfg(not(windows))]
    path.to_string_lossy().into_owned()
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
        return Err(crate::i18n::text(
            "backend-setup-clones-require-local-absolute-folders-a23011",
        )
        .into());
    }
    #[cfg(windows)]
    let source_path = local.then(|| setup_path(Path::new(source)));
    #[cfg(windows)]
    let source = source_path.as_deref().unwrap_or(source);
    #[cfg(windows)]
    let destination = setup_path(destination);
    let mut prepared = transport_command(source, local, executable)?;
    prepared
        .command
        .args([
            "clone",
            "--no-checkout",
            "--template=",
            "--origin",
            remote,
            "--",
            source,
        ])
        .arg(destination);
    Ok(prepared)
}

fn fetch_command(
    repo: &Path,
    source: &str,
    remote: &str,
    executable: &Path,
) -> Result<CloneCommand, String> {
    if crate::remote::is_remote(repo) || !repo.is_absolute() {
        return Err(crate::i18n::text(
            "backend-setup-fetches-require-a-newly-imported-local-repository-7bbb73",
        )
        .into());
    }
    crate::portability::reference(remote)?;
    let mut prepared = transport_command(source, false, executable)?;
    prepared
        .command
        .arg("--git-dir")
        .arg(repo.join(".git"))
        .arg("fetch")
        .args(["--no-tags", "--no-write-fetch-head", "--", source])
        .arg(format!("+refs/heads/*:refs/remotes/{remote}/*"))
        .arg(format!("+refs/tags/*:refs/gitcraft-setup/{remote}/tags/*"));
    Ok(prepared)
}

/// Secondary remotes get the same pinned URL and explicit account context as the clone.
/// Never use the newly written remote name to resolve a URL or an ambient credential.
pub(crate) fn setup_fetch(
    repo: &Path,
    source: &str,
    remote: &str,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|_| {
        crate::i18n::text(
            "backend-could-not-locate-the-selected-profile-authentication-helper-63e669",
        )
    })?;
    let CloneCommand {
        command,
        workspace: _workspace,
    } = fetch_command(repo, source, remote, &executable)?;
    let out = setup_process(command, active, Duration::from_secs(1800))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(crate::i18n::message(
            "backend-secondary-remote-remote-could-not-be-fetched-with-this-profile-s-f7f7b0",
            &[("remote", (remote).to_string().into())],
        ))
    }
}

pub(crate) fn setup_clone(
    parent: &Path,
    source: &str,
    destination: &Path,
    remote: &str,
    local: bool,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|_| {
        crate::i18n::text(
            "backend-could-not-locate-the-selected-profile-authentication-helper-63e669",
        )
    })?;
    let CloneCommand {
        command,
        workspace: _workspace,
    } = clone_command(parent, source, destination, remote, local, &executable)?;
    let out = setup_process(command, active, Duration::from_secs(1800))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(crate::i18n::text(
            "backend-the-previewed-source-could-not-be-cloned-with-this-profile-s-expl-cc87af",
        )
        .into())
    }
}

#[cfg(test)]
mod tests;

fn setup_process(
    command: Command,
    active: &dyn Fn() -> bool,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    if !active() {
        return Err("Setup operation cancelled".into());
    }
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            background: true,
            timeout: Some(timeout),
            output_limit: 64 << 20,
            ..Default::default()
        },
    )?
    .complete()?;
    if !active() {
        return Err("Setup operation cancelled".into());
    }
    Ok(out)
}
/// Isolated writes are restricted to the newly created, receipt-verified import.
pub(crate) fn setup_run(
    repo: &Path,
    args: &[&str],
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text("backend-setup-targets-local-folders-only-e1dfde").into());
    }
    let mut command = practice_command(repo);
    command
        .args([
            "--no-lazy-fetch",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "protocol.file.allow=never",
        ])
        .args(args);
    let out = setup_process(command, active, Duration::from_secs(120))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(crate::i18n::message(
            "backend-setup-step-failed-f3d4fe",
            &[(
                "crate",
                (crate::diagnostics::crash::scrub(&String::from_utf8_lossy(&out.stderr)))
                    .to_string()
                    .into(),
            )],
        ))
    }
}
