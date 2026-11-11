//! Git-global config discovery and profile config serialization.

use super::{UserProfile, signing};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The files git reads as the global config when nothing stands in for them, in the order it
/// reads them (the later one wins): `GIT_CONFIG_GLOBAL` when gitcraft was started with it set,
/// otherwise `$XDG_CONFIG_HOME/git/config` and `~/.gitconfig`.
pub fn user_global_files() -> Vec<PathBuf> {
    global_files_from(|key| std::env::var_os(key))
}

pub(super) fn global_files_from(env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let var = |key: &str| env(key).filter(|v| !v.is_empty());
    if let Some(file) = var("GIT_CONFIG_GLOBAL") {
        return vec![PathBuf::from(file)];
    }
    // Where HOME isn't set, Git for Windows takes HOMEDRIVE + HOMEPATH when that folder is
    // there (a home share), else USERPROFILE.
    let home_share = || {
        if !cfg!(windows) {
            return None;
        }
        let mut drive = var("HOMEDRIVE")?;
        drive.push(var("HOMEPATH")?);
        Some(drive).filter(|dir| Path::new(dir).is_dir())
    };
    let home = var("HOME")
        .or_else(home_share)
        .or_else(|| var("USERPROFILE"))
        .map(PathBuf::from);
    let xdg = var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|h| h.join(".config")))
        .map(|d| d.join("git").join("config"));
    xdg.into_iter()
        .chain(home.map(|h| h.join(".gitconfig")))
        .collect()
}

/// `value` as a quoted git config value: backslashes and quotes escaped, no line breaks (they
/// can't be in a name, an email or a path anyway).
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\t' => out.push_str("\\t"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `key = value` under its own section header: `gpg.ssh.program` goes under `[gpg "ssh"]`.
pub(super) fn entry(key: &str, value: &str) -> String {
    let (section, name) = key.rsplit_once('.').unwrap_or(("core", key));
    let header = match section.split_once('.') {
        Some((section, sub)) => format!("[{section} {}]", quote(sub)),
        None => format!("[{section}]"),
    };
    format!("{header}\n\t{name} = {}\n", quote(value))
}

/// The config file for `profile`: the user's own global config files (`includes`) first, then
/// what the profile sets, which wins over them.
pub fn config_text(profile: &UserProfile, includes: &[PathBuf]) -> String {
    let mut text = format!(
        "# Written by gitcraft for its profile {}; it is written over whenever the profile\n\
         # changes. Change the profile in gitcraft's settings, or your own config in the files\n\
         # included here. A repository's own config wins over both.\n",
        quote(profile.title())
    );
    if !includes.is_empty() {
        // A file that isn't there is skipped by git.
        text.push_str("[include]\n");
        for file in includes {
            text.push_str(&format!("\tpath = {}\n", quote(&file.to_string_lossy())));
        }
    }
    let name = profile.author_name.trim();
    if !name.is_empty() {
        text.push_str(&entry("user.name", name));
    }
    let email = profile.author_email.trim();
    if !email.is_empty() {
        text.push_str(&entry("user.email", email));
    }
    if !profile.connections.ssh_key.is_empty()
        && let Ok(exe) = std::env::current_exe()
    {
        // The installed helper speaks OpenSSH even if the included user config selects
        // PuTTY. Auto probes this wrapper as OpenSSH and still detects a repository-local
        // Plink override by its executable name; local ssh.variant retains precedence.
        text.push_str(&entry("ssh.variant", "auto"));
        text.push_str(&entry(
            "core.sshCommand",
            &crate::connections::ssh_helper_command(&profile.connections.ssh_key, &exe),
        ));
    }
    if let Ok(exe) = std::env::current_exe() {
        for (tool, account) in [
            ("gh", &profile.connections.github),
            ("glab", &profile.connections.gitlab),
        ] {
            if account.selected() {
                let section = format!("credential.https://{}.helper", account.host);
                text.push_str(&entry(&section, ""));
                text.push_str(&entry(
                    &section,
                    &crate::connections::helper_command(tool, account, &exe),
                ));
            }
        }
    }
    if !profile.connections.tea_login.is_empty()
        && !profile.connections.tea_host.is_empty()
        && let Ok(exe) = std::env::current_exe()
    {
        let account = crate::connections::Account {
            host: profile.connections.tea_host.clone(),
            user: profile.connections.tea_login.clone(),
            config_dir: String::new(),
        };
        let section = format!("credential.https://{}.helper", account.host);
        text.push_str(&entry(&section, ""));
        text.push_str(&entry(
            &section,
            &crate::connections::helper_command("tea", &account, &exe),
        ));
    }
    if profile.own_signing {
        let s = &profile.signing;
        let flag = |on: bool| if on { "true" } else { "false" };
        text.push_str(&entry("gpg.format", s.format()));
        // Empty: what the user's own config says, or git's default.
        if !s.key.trim().is_empty() {
            text.push_str(&entry("user.signingkey", s.key.trim()));
        }
        if !s.program.trim().is_empty() {
            text.push_str(&entry(signing::program_key(s.format()), s.program.trim()));
        }
        text.push_str(&entry("commit.gpgSign", flag(s.sign_commits)));
        text.push_str(&entry("tag.gpgSign", flag(s.sign_tags)));
    }
    text
}
