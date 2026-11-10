//! Profiles: sets of settings to switch between, each with its own git author, commit signing,
//! theme and open tabs (the tabs are the app's, see `app::profiles`).
//!
//! What a profile sets for git goes into a config file of its own that git reads as the global
//! config (`GIT_CONFIG_GLOBAL`). That file first includes the user's own global config, then
//! sets the profile's values, so they win over `~/.gitconfig`. It is still the global scope:
//! what a repository sets itself (`git config --local`, `--worktree`) wins over the profile,
//! as it does over `~/.gitconfig`.

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
    /// Expected outgoing author by host or host/namespace; used by push preflight.
    pub author_hosts: Vec<HostAuthor>,
    pub connections: crate::connections::Choices,
    /// Imported setup requirements must be verified on this machine before use.
    pub pending_setup: Option<crate::portability::ConnectionRequirements>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostAuthor {
    pub host: String,
    pub name: String,
    pub email: String,
}

impl Default for UserProfile {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Default".to_owned(),
            author_name: String::new(),
            author_email: String::new(),
            pending_setup: None,
            own_signing: false,
            signing: Signing::default(),
            author_hosts: Vec::new(),
            connections: crate::connections::Choices {
                github: crate::connections::Account::on("github.com"),
                gitlab: crate::connections::Account::on("gitlab.com"),
                ..Default::default()
            },
        }
    }
}

impl UserProfile {
    pub(crate) fn expected_author(&self, url: &str) -> (&str, &str) {
        let scheme = url.split_once("://").map_or("ssh", |(scheme, _)| scheme);
        let policy_host = |host: &str| {
            crate::push_preflight::location(&format!("{scheme}://{host}/_")).map(|(host, _)| host)
        };
        let policy = crate::push_preflight::location(url).and_then(|(host, path)| {
            let path = path.to_ascii_lowercase();
            self.author_hosts
                .iter()
                .filter_map(|p| {
                    let key = p.host.trim().to_ascii_lowercase();
                    let (h, namespace) = key.split_once('/').unwrap_or((&key, ""));
                    let namespace = namespace.trim_end_matches('/');
                    (policy_host(h).as_deref() == Some(host.as_str())
                        && (namespace.is_empty()
                            || path
                                .strip_prefix(namespace)
                                .is_some_and(|tail| tail.is_empty() || tail.starts_with('/'))))
                    .then_some((p, namespace.len()))
                })
                // Host spelling (default ports, aliases, or whitespace) must not
                // outweigh a more specific namespace. Match and rank the same
                // namespace, ignoring directory suffixes. Equal ranks keep the last rule.
                .max_by_key(|(_, specificity)| *specificity)
                .map(|(p, _)| p)
        });
        policy.map_or(
            (self.author_name.as_str(), self.author_email.as_str()),
            |p| (p.name.as_str(), p.email.as_str()),
        )
    }

    /// Whether it sets anything in git's config.
    pub fn pending_setup(&self) -> bool {
        self.pending_setup.as_ref().is_some_and(|p| p.pending())
    }

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

    /// The visible title; the raw title remains stable for generated config and saved names.
    pub fn display_title(&self) -> &str {
        if self.name.trim().is_empty() {
            crate::i18n::text("interaction-accounts-unnamed")
        } else {
            self.title()
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

mod config;
mod runtime;

pub use config::{config_text, user_global_files};
pub use runtime::{apply, error, forget};

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
#[path = "profiles/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "profiles/connection_tests.rs"]
mod connection_tests;
/// A profile's git config in a portable data folder, including spaces and non-ASCII letters,
/// is what git reads there.
#[cfg(test)]
#[path = "profiles/portable_tests.rs"]
mod portable_tests;
