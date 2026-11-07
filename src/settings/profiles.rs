//! Profiles: sets of settings to switch between, each with its own git author, commit signing,
//! theme and open tabs (the tabs are the app's, see `app::profiles`).
//!
//! What a profile sets for git goes into a config file of its own that git reads as the global
//! config (`GIT_CONFIG_GLOBAL`). That file first includes the user's own global config, then
//! sets the profile's values, so they win over `~/.gitconfig`. It is still the global scope:
//! what a repository sets itself (`git config --local`, `--worktree`) wins over the profile,
//! as it does over `~/.gitconfig`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::signing::{self, Signing};

/// A profile, as the settings keep it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserProfile {
    /// Tells profiles apart for good, whatever they are renamed to.
    pub id: u64,
    pub name: String,
    /// `user.name` for this profile; empty leaves the global git config's.
    pub author_name: String,
    /// `user.email` for this profile; empty leaves the global git config's.
    pub author_email: String,
    /// Sign as `signing` says rather than as the global git config does.
    pub own_signing: bool,
    pub signing: Signing,
    pub connections: crate::connections::Choices,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Default".to_owned(),
            author_name: String::new(),
            author_email: String::new(),
            own_signing: false,
            signing: Signing::default(),
            connections: crate::connections::Choices {
                github: crate::connections::Account::on("github.com"),
                gitlab: crate::connections::Account::on("gitlab.com"),
                ..Default::default()
            },
        }
    }
}

impl UserProfile {
    /// Whether it sets anything in git's config.
    pub fn sets_git(&self) -> bool {
        !self.author_name.trim().is_empty()
            || !self.author_email.trim().is_empty()
            || self.own_signing
            || !self.connections.ssh_key.is_empty()
            || self.connections.github.selected()
            || self.connections.gitlab.selected()
            || !self.connections.tea_login.is_empty()
    }

    /// Its name, or a stand-in when it has none.
    pub fn title(&self) -> &str {
        let name = self.name.trim();
        if name.is_empty() {
            "Unnamed profile"
        } else {
            name
        }
    }

    /// One or two letters for its badge: the first letters of its first two words.
    pub fn initials(&self) -> String {
        let letters: String = self
            .title()
            .split_whitespace()
            .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        if letters.is_empty() {
            "?".to_owned()
        } else {
            letters
        }
    }
}

/// The files git reads as the global config when nothing stands in for them, in the order it
/// reads them (the later one wins): `GIT_CONFIG_GLOBAL` when gitcraft was started with it set,
/// otherwise `$XDG_CONFIG_HOME/git/config` and `~/.gitconfig`.
pub fn user_global_files() -> Vec<PathBuf> {
    global_files_from(|key| std::env::var_os(key))
}

fn global_files_from(env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
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
fn entry(key: &str, value: &str) -> String {
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
        // PuTTY. Repository-local ssh.variant/core.sshCommand still have normal precedence.
        text.push_str(&entry("ssh.variant", "ssh"));
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

/// Where the config file of profile `id` goes.
fn config_path(id: u64) -> Option<PathBuf> {
    eframe::storage_dir("gitcraft").map(|d| profile_file(&d, id))
}

fn profile_file(storage: &Path, id: u64) -> PathBuf {
    storage.join("profiles").join(format!("{id}.gitconfig"))
}

/// The file written last and what was in it, so that it is written again only when it changes.
static WRITTEN: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

/// Why the active profile's config couldn't be written, if it couldn't.
static ERROR: Mutex<Option<String>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Writes `text` to `path` through a file next to it, so git never reads half of it.
fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("gitconfig.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Has gitcraft's git commands go by `profile` from now on: writes its config file (when it
/// changed) and points git at it, or back at the user's own config when it sets nothing.
pub fn apply(profile: &UserProfile) {
    let mut context = crate::connections::Context {
        choices: profile.connections.clone(),
        ..Default::default()
    };
    let result = profile.connections.validate().and_then(|_| {
        if profile.sets_git() && std::env::current_exe().is_err() {
            return Err("Could not locate gitcraft's connection helpers".to_owned());
        }
        if !profile.sets_git() {
            return Ok(None);
        }
        config_path(profile.id)
            .ok_or_else(|| "No folder to keep the profile's git config in".to_owned())
            .and_then(|path| {
                let text = config_text(profile, &user_global_files());
                let mut written = lock(&WRITTEN);
                let fresh = written
                    .as_ref()
                    .is_some_and(|(p, t)| *p == path && *t == text)
                    && path.is_file();
                if !fresh {
                    write_file(&path, &text)?;
                    *written = Some((path.clone(), text.clone()));
                }
                static SESSION: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
                let session = SESSION.get_or_init(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                });
                let snapshot = path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("snapshots")
                    .join(format!(
                        "{}-{}-{session}-{:x}.gitconfig",
                        profile.id,
                        std::process::id(),
                        md5::compute(text.as_bytes())
                    ));
                let lease = crate::connections::ConfigLease::acquire(snapshot.clone());
                if !snapshot.is_file() {
                    write_file(&snapshot, &text)?;
                }
                Ok(Some(lease))
            })
    });
    match result {
        Ok(lease) => {
            context.git_config = lease.as_ref().map(|lease| lease.path.clone());
            context.lease = lease;
            *lock(&ERROR) = None;
        }
        Err(error) => {
            context.error = Some(error.clone());
            *lock(&ERROR) = Some(format!(
                "The profile's connection settings aren't in use: {error}"
            ));
        }
    }
    crate::connections::set(context);
}

/// Why the active profile's git settings aren't in use, if they aren't.
pub fn error() -> Option<String> {
    lock(&ERROR).clone()
}

/// Deletes the config file of a profile that was deleted.
pub fn forget(id: u64) {
    if let Some(path) = config_path(id) {
        let _ = std::fs::remove_file(path);
    }
}

/// Whether `git --version`'s answer is a git that reads `GIT_CONFIG_GLOBAL` (2.32 and later).
pub fn git_reads_profiles(version: &str) -> bool {
    let numbers: Vec<u32> = version
        .split_whitespace()
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or_default()
        .split('.')
        .map_while(|n| n.parse().ok())
        .collect();
    match numbers.as_slice() {
        [major, minor, ..] => (*major, *minor) >= (2, 32),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn global_files_are_found_as_git_finds_them() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        assert_eq!(
            global_files_from(env(&[("HOME", "/home/me")])),
            [
                PathBuf::from("/home/me/.config/git/config"),
                PathBuf::from("/home/me/.gitconfig")
            ]
        );
        assert_eq!(
            global_files_from(env(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "/x")])),
            [
                PathBuf::from("/x/git/config"),
                PathBuf::from("/h/.gitconfig")
            ]
        );
        assert_eq!(
            global_files_from(env(&[("HOME", ""), ("USERPROFILE", "C:/Users/me")])),
            [
                PathBuf::from("C:/Users/me/.config/git/config"),
                PathBuf::from("C:/Users/me/.gitconfig")
            ]
        );
        // Started with one of its own: only that one, as git would.
        assert_eq!(
            global_files_from(env(&[("HOME", "/h"), ("GIT_CONFIG_GLOBAL", "/etc/mine")])),
            [PathBuf::from("/etc/mine")]
        );
    }

    #[test]
    fn initials_and_titles() {
        let named = |name: &str| UserProfile {
            name: name.to_owned(),
            ..UserProfile::default()
        };
        assert_eq!(named("Work").initials(), "W");
        assert_eq!(named("open source stuff").initials(), "OS");
        assert_eq!(named("  ").initials(), "UP");
        assert_eq!(named("  ").title(), "Unnamed profile");
        assert_eq!(named("(x) ümlaut").initials(), "XÜ");
    }

    #[test]
    fn a_profile_sets_only_what_it_has() {
        let plain = UserProfile::default();
        assert!(!plain.sets_git());
        let text = config_text(&plain, &[]);
        assert!(
            !text.contains("[user]") && !text.contains("[include]"),
            "{text}"
        );

        let work = UserProfile {
            name: "Work \"A\"".to_owned(),
            author_name: " Ann O'Neil ".to_owned(),
            author_email: "ann@corp.example".to_owned(),
            own_signing: true,
            signing: Signing {
                format: "ssh".to_owned(),
                key: "C:\\keys\\id.pub".to_owned(),
                program: String::new(),
                sign_commits: true,
                sign_tags: false,
            },
            ..UserProfile::default()
        };
        assert!(work.sets_git());
        let text = config_text(&work, &[PathBuf::from("C:\\Users\\ann\\.gitconfig")]);
        assert!(
            text.contains("\tpath = \"C:\\\\Users\\\\ann\\\\.gitconfig\"\n"),
            "{text}"
        );
        assert!(text.contains("[user]\n\tname = \"Ann O'Neil\"\n"), "{text}");
        assert!(
            text.contains("\tsigningkey = \"C:\\\\keys\\\\id.pub\"\n"),
            "{text}"
        );
        assert!(text.contains("[gpg]\n\tformat = \"ssh\"\n"), "{text}");
        assert!(text.contains("[commit]\n\tgpgSign = \"true\"\n"), "{text}");
        assert!(text.contains("[tag]\n\tgpgSign = \"false\"\n"), "{text}");
        // No program: whatever the user's own config says.
        assert!(!text.contains("program"), "{text}");
        assert!(text.starts_with("# Written by gitcraft for its profile \"Work \\\"A\\\"\""));
        assert_eq!(
            entry("gpg.ssh.program", "/usr/bin/ssh-keygen"),
            "[gpg \"ssh\"]\n\tprogram = \"/usr/bin/ssh-keygen\"\n"
        );
    }

    #[test]
    fn git_versions_that_read_profiles() {
        assert!(git_reads_profiles("git version 2.43.0"));
        assert!(git_reads_profiles("git version 2.32.0.windows.1"));
        assert!(git_reads_profiles("git version 3.0"));
        assert!(!git_reads_profiles("git version 2.31.8"));
        assert!(!git_reads_profiles("git version 1.9.5.msysgit.0"));
        assert!(git_reads_profiles("something else"));
    }

    /// The profile wins over the user's own global config, which it includes, and a
    /// repository's own config wins over the profile. Against scratch files, never the user's.
    #[test]
    fn a_repository_wins_over_the_profile_which_wins_over_the_global_config() -> Result<(), String>
    {
        let text = |e: std::io::Error| e.to_string();
        let dir = std::env::temp_dir().join(format!("gitcraft-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).map_err(text)?;
        let own = dir.join("own.gitconfig");
        std::fs::write(
            &own,
            "[user]\n\tname = Own Name\n\temail = own@example.com\n[core]\n\tfrom-own = yes\n",
        )
        .map_err(text)?;
        let profile = UserProfile {
            name: "Work".to_owned(),
            author_email: "work@example.com".to_owned(),
            ..UserProfile::default()
        };
        let file = profile_file(&dir, profile.id);
        let missing = dir.join("not-there");
        write_file(&file, &config_text(&profile, &[missing, own.clone()]))?;
        let git = |args: &[&str]| -> Result<String, String> {
            let out = Command::new("git")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", &file)
                .current_dir(&repo)
                .args(args)
                .output()
                .map_err(text)?;
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
            }
        };
        let result = (|| {
            git(&["init", "-q"])?;
            let before = (
                git(&["config", "user.name"])?,
                git(&["config", "user.email"])?,
                git(&["config", "core.from-own"])?,
            );
            git(&["config", "--local", "user.email", "repo@example.com"])?;
            let after = git(&["config", "user.email"])?;
            Ok::<_, String>((before, after))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        let (before, after) = result?;
        // The name the profile leaves alone, and the rest of the user's own config, stay.
        assert_eq!(
            before,
            (
                "Own Name".to_owned(),
                "work@example.com".to_owned(),
                "yes".to_owned()
            )
        );
        assert_eq!(after, "repo@example.com");
        Ok(())
    }
}

#[cfg(test)]
mod connection_tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn ssh_key_and_host_helpers_round_trip_through_git_config() -> Result<(), String> {
        let directory = std::env::temp_dir().join(format!(
            "gitcraft-profile-key-config-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let key = directory.join("work 'key' $(not-a-command); `literal` \\");
        let mut profile = UserProfile::default();
        profile.connections.ssh_key = key.to_string_lossy().into_owned();
        profile.connections.github.user = "work".into();
        profile.author_name = "Work author".into();
        let text = config_text(&profile, &[]);
        let file = directory.join("profile.gitconfig");
        std::fs::write(&file, &text).map_err(|e| e.to_string())?;
        let get = |name: &str| -> Result<String, String> {
            let out = Command::new("git")
                .args(["config", "--file"])
                .arg(&file)
                .args(["--get-all", name])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
        };
        assert_eq!(
            get("core.sshCommand")?,
            crate::connections::ssh_helper_command(
                &profile.connections.ssh_key,
                &std::env::current_exe().map_err(|e| e.to_string())?
            )
        );
        assert_eq!(get("user.name")?, "Work author");
        assert_eq!(get("ssh.variant")?, "ssh");
        let helpers = get("credential.https://github.com.helper")?;
        assert!(helpers.starts_with('\n'));
        assert!(helpers.contains("--gitcraft-profile-credential gh "));
        assert!(!text.contains("password="));
        let _ = std::fs::remove_dir_all(directory);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn selected_openssh_profile_overrides_inherited_putty_variant_and_keeps_nondefault_port()
    -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-profile-ssh-variant-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .arg(&repo)
                .status()
                .map_err(|e| e.to_string())?
                .success()
        );
        let key = dir.join("selected key");
        std::fs::write(&key, "").map_err(|e| e.to_string())?;
        let recorder = dir.join("record-ssh");
        let args_file = dir.join("args");
        let effective = dir.join("effective");
        // This transport only records Git's arguments and asks real OpenSSH to parse them.
        // -G never connects, and stdout is kept away from Git's protocol parser.
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nssh -F /dev/null -G -o IdentitiesOnly=yes -i {} \"$@\" > {} 2>/dev/null\nexit 1\n",
            crate::connections::shell_quote(&args_file.to_string_lossy()),
            crate::connections::shell_quote(&key.to_string_lossy()),
            crate::connections::shell_quote(&effective.to_string_lossy())
        );
        std::fs::write(&recorder, script).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&recorder, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let global = dir.join("user.gitconfig");
        let config = dir.join("profile.gitconfig");
        let mut profile = UserProfile::default();
        profile.connections.ssh_key = key.to_string_lossy().into_owned();
        for variant in ["plink", "tortoiseplink", "putty", "simple"] {
            std::fs::write(&global, entry("ssh.variant", variant)).map_err(|e| e.to_string())?;
            let mut text = config_text(&profile, std::slice::from_ref(&global));
            // Replace only the executable with the recording transport, retaining profile
            // variant generation and production Git invocation/context handling.
            text.push_str(&entry(
                "core.sshCommand",
                &crate::connections::shell_quote(&recorder.to_string_lossy()),
            ));
            std::fs::write(&config, text).map_err(|e| e.to_string())?;
            let context = crate::connections::Context {
                git_config: Some(config.clone()),
                choices: profile.connections.clone(),
                ..Default::default()
            };
            assert!(
                context
                    .run(|| crate::git::run(
                        Some(&repo),
                        &["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"]
                    ))
                    .is_err()
            );
            let args = std::fs::read_to_string(&args_file).map_err(|e| e.to_string())?;
            assert!(
                args.lines()
                    .collect::<Vec<_>>()
                    .windows(2)
                    .any(|pair| pair == ["-p", "2222"]),
                "{variant}: {args}"
            );
            assert!(!args.lines().any(|arg| arg == "-P"), "{variant}: {args}");
            let parsed = std::fs::read_to_string(&effective).map_err(|e| e.to_string())?;
            assert!(
                parsed.lines().any(|line| line == "port 2222"),
                "{variant}: {parsed}"
            );
        }
        // Git's repository-local policy still has precedence over the generated profile.
        assert!(
            Command::new("git")
                .current_dir(&repo)
                .args(["config", "ssh.variant", "plink"])
                .status()
                .map_err(|e| e.to_string())?
                .success()
        );
        let context = crate::connections::Context {
            git_config: Some(config),
            choices: profile.connections,
            ..Default::default()
        };
        assert!(
            context
                .run(|| crate::git::run(
                    Some(&repo),
                    &["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"]
                ))
                .is_err()
        );
        let args = std::fs::read_to_string(args_file).map_err(|e| e.to_string())?;
        assert!(
            args.lines()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| pair == ["-P", "2222"]),
            "{args}"
        );
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }
}
