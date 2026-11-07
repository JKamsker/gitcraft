//! One versioned, allowlisted setup file. Credentials, executable commands, raw Git config,
//! auth folders, private/signing key values and old machine paths never become active config.
mod repository;
mod restore;
#[cfg(test)]
mod tests;
mod url;
use crate::{
    settings::{SavedIdentity, Settings, UserProfile},
    theme::ThemeId,
};
pub(crate) use repository::snapshot;
pub(crate) use restore::{Outcome, Receipt, restore};
use serde::{Deserialize, Serialize};
pub(crate) use url::sanitize_url;

pub(crate) const FILE_LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Github,
    Gitlab,
    Tea,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicAccountExpectation {
    pub provider: Provider,
    pub host: String,
    pub user_or_alias: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionRequirements {
    pub ssh_key_reselection_required: bool,
    pub signing_key_reselection_required: bool,
    pub accounts: Vec<PublicAccountExpectation>,
}
impl ConnectionRequirements {
    pub fn pending(&self) -> bool {
        self.ssh_key_reselection_required
            || self.signing_key_reselection_required
            || !self.accounts.is_empty()
    }
}

macro_rules! preferences{($($field:ident:$ty:ty),*$(,)?)=>{
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]#[serde(deny_unknown_fields)]
pub(crate) struct Preferences{$(pub $field:$ty,)*}
impl From<&Settings> for Preferences{fn from(s:&Settings)->Self{Self{$($field:s.$field.clone(),)*}}}
impl Preferences{pub fn apply(&self,s:&mut Settings){$(s.$field=self.$field.clone();)*}}
};}
preferences! {ui_scale:f32,terminal_font:f32,refs_width:f32,auto_refs_width:bool,graph_width:f32,compact_graph:bool,author_width:f32,when_width:f32,date_width:f32,hash_width:f32,show_author:bool,show_when:bool,show_date:bool,show_hash:bool,show_branches:bool,show_detail:bool,worktrees_in_same_tab:bool,files_as_tree:bool,diff_split:bool,diff_whole_file:bool,notifications:bool,theme_tweaks:crate::theme::Tweaks}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicProfile {
    pub source_id: u64,
    pub name: String,
    pub author_name: String,
    pub author_email: String,
    pub theme: String,
    pub own_signing: bool,
    pub signing_format: String,
    pub sign_commits: bool,
    pub sign_tags: bool,
    pub setup: ConnectionRequirements,
}
impl PublicProfile {
    pub fn capture(p: &UserProfile, theme: &ThemeId) -> Self {
        let mut setup = p.pending_setup.clone().unwrap_or_default();
        setup.ssh_key_reselection_required |= !p.connections.ssh_key.is_empty();
        setup.signing_key_reselection_required |=
            p.own_signing && (p.signing.sign_commits || p.signing.sign_tags);
        if p.pending_setup.is_none() {
            for (provider, account) in [
                (Provider::Github, &p.connections.github),
                (Provider::Gitlab, &p.connections.gitlab),
            ] {
                if account.selected() {
                    setup.accounts.push(PublicAccountExpectation {
                        provider,
                        host: account.host.clone(),
                        user_or_alias: account.user.clone(),
                    });
                }
            }
            if !p.connections.tea_login.is_empty() {
                setup.accounts.push(PublicAccountExpectation {
                    provider: Provider::Tea,
                    host: p.connections.tea_host.clone(),
                    user_or_alias: p.connections.tea_login.clone(),
                });
            }
        }
        Self {
            source_id: p.id,
            name: p.name.clone(),
            author_name: p.author_name.clone(),
            author_email: p.author_email.clone(),
            theme: if crate::theme::portable_theme(theme) {
                theme.0.clone()
            } else {
                ThemeId::default().0
            },
            own_signing: p.own_signing,
            signing_format: p.signing.format.clone(),
            sign_commits: p.signing.sign_commits,
            sign_tags: p.signing.sign_tags,
            setup,
        }
    }
    pub fn materialize(&self, id: u64) -> UserProfile {
        let mut p = UserProfile {
            id,
            name: self.name.clone(),
            author_name: self.author_name.clone(),
            author_email: self.author_email.clone(),
            own_signing: self.own_signing,
            ..Default::default()
        };
        p.signing.format = self.signing_format.clone();
        p.signing.sign_commits = self.sign_commits;
        p.signing.sign_tags = self.sign_tags;
        p.pending_setup = self.setup.pending().then(|| self.setup.clone());
        p
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub label: String,
    pub name: String,
    pub email: String,
}
impl From<&SavedIdentity> for Identity {
    fn from(i: &SavedIdentity) -> Self {
        Self {
            label: i.label.clone(),
            name: i.name.clone(),
            email: i.email.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Remote {
    pub name: String,
    pub fetch: String,
    pub push: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Worktree {
    pub name: String,
    pub previous_path: String,
    pub branch: Option<String>,
    pub commit: String,
    pub was_locked: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Repository {
    pub name: String,
    pub previous_path: String,
    pub profile_ids: Vec<u64>,
    pub remotes: Vec<Remote>,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub hidden: Vec<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub worktrees: Vec<Worktree>,
    pub notices: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Setup {
    pub format: String,
    pub version: u32,
    pub preferences: Preferences,
    pub profiles: Vec<PublicProfile>,
    pub identities: Vec<Identity>,
    pub repositories: Vec<Repository>,
}
impl Setup {
    pub fn new(settings: &Settings, themes: &std::collections::BTreeMap<u64, ThemeId>) -> Self {
        Self {
            format: "gitcraft-setup".into(),
            version: 1,
            preferences: settings.into(),
            profiles: settings
                .user_profiles
                .iter()
                .map(|p| {
                    PublicProfile::capture(p, themes.get(&p.id).unwrap_or(&ThemeId::default()))
                })
                .collect(),
            identities: settings.identities.iter().map(Identity::from).collect(),
            repositories: Vec::new(),
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > FILE_LIMIT {
            return Err("Setup file exceeds 8 MiB".into());
        }
        let file: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid setup file: {e}"))?;
        file.validate()?;
        Ok(file)
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() > FILE_LIMIT {
            return Err("Setup file exceeds 8 MiB".into());
        }
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.format != "gitcraft-setup" {
            return Err("Unsupported setup format/version".into());
        }
        if self.profiles.len() > 64 || self.identities.len() > 256 || self.repositories.len() > 64 {
            return Err("Setup contains too many profiles, identities or repositories".into());
        }
        for value in [
            self.preferences.ui_scale,
            self.preferences.terminal_font,
            self.preferences.refs_width,
            self.preferences.graph_width,
            self.preferences.author_width,
            self.preferences.when_width,
            self.preferences.date_width,
            self.preferences.hash_width,
            self.preferences.theme_tweaks.roundness,
        ] {
            if !value.is_finite() || !(0.0..=2000.0).contains(&value) {
                return Err("Invalid display setting".into());
            }
        }
        if !(0.5..=3.0).contains(&self.preferences.ui_scale)
            || !(8.0..=32.0).contains(&self.preferences.terminal_font)
            || !(0.0..=2.0).contains(&self.preferences.theme_tweaks.roundness)
        {
            return Err("Display setting is outside supported range".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for p in &self.profiles {
            if !ids.insert(p.source_id) {
                return Err("Duplicate profile ID".into());
            }
            for s in [&p.name, &p.author_name, &p.author_email] {
                text(s, 512)?;
            }
            if !crate::theme::portable_theme(&ThemeId(p.theme.clone())) {
                return Err("Only built-in themes can be imported".into());
            }
            if !["", "openpgp", "ssh", "x509"].contains(&p.signing_format.as_str()) {
                return Err("Unsupported signing format".into());
            }
            if p.setup.accounts.len() > 3 {
                return Err("Too many account expectations".into());
            }
            for a in &p.setup.accounts {
                text(&a.user_or_alias, 256)?;
                host(&a.host)?;
                if a.user_or_alias.is_empty() {
                    return Err("Expected account is empty".into());
                }
            }
        }
        for i in &self.identities {
            for s in [&i.label, &i.name, &i.email] {
                text(s, 512)?;
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for r in &self.repositories {
            leaf(&r.name)?;
            if !names.insert(&r.name) {
                return Err("Duplicate repository destination name".into());
            }
            text(&r.previous_path, 4096)?;
            if r.profile_ids.iter().any(|id| !ids.contains(id)) {
                return Err("Repository references an unknown profile".into());
            }
            if r.remotes.len() > 32
                || r.worktrees.len() > 64
                || r.hidden.len() > 2000
                || r.notices.len() > 64
            {
                return Err("Repository metadata exceeds limits".into());
            }
            for remote in &r.remotes {
                reference(&remote.name)?;
                if sanitize_url(&remote.fetch)?.0 != remote.fetch {
                    return Err("Remote URL contains credentials or nonportable decorations".into());
                }
                if remote.push.len() > 16 {
                    return Err("Too many push URLs".into());
                }
                for u in &remote.push {
                    if sanitize_url(u)?.0 != *u {
                        return Err("Push URL contains credentials".into());
                    }
                }
            }
            for s in [&r.author_name, &r.author_email].into_iter().flatten() {
                text(s, 512)?;
            }
            for s in &r.hidden {
                full_ref(s)?;
            }
            if let Some(b) = &r.branch {
                reference(b)?;
            }
            if let Some(c) = &r.commit {
                oid(c)?;
            }
            let mut worktrees = std::collections::BTreeSet::new();
            for w in &r.worktrees {
                leaf(&w.name)?;
                if !worktrees.insert(&w.name) {
                    return Err("Duplicate worktree name".into());
                }
                text(&w.previous_path, 4096)?;
                if let Some(b) = &w.branch {
                    reference(b)?;
                }
                oid(&w.commit)?;
            }
            for n in &r.notices {
                text(n, 1024)?;
            }
        }
        Ok(())
    }
}
fn text(s: &str, max: usize) -> Result<(), String> {
    if s.len() > max || s.chars().any(char::is_control) {
        Err("Text contains control characters or exceeds its limit".into())
    } else {
        Ok(())
    }
}
pub(crate) fn leaf(s: &str) -> Result<(), String> {
    text(s, 100)?;
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.ends_with(['.', ' '])
        || s.starts_with('.')
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || ["con", "prn", "aux", "nul", "com1", "lpt1"].contains(&s.to_ascii_lowercase().as_str())
    {
        return Err("Choose a plain, unique folder name".into());
    }
    Ok(())
}
fn reference(s: &str) -> Result<(), String> {
    text(s, 256)?;
    if s.is_empty()
        || s.starts_with(['-', '.', '/'])
        || s.ends_with(['.', '/'])
        || s.contains("..")
        || s.contains("//")
        || s.contains("@{")
        || s.ends_with(".lock")
        || s.split('/').any(|p| p.starts_with('.'))
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
    {
        return Err("Invalid portable ref or remote name".into());
    }
    Ok(())
}
fn full_ref(s: &str) -> Result<(), String> {
    if !s.starts_with("refs/heads/") && !s.starts_with("refs/remotes/") {
        return Err("Only branch visibility refs are portable".into());
    }
    reference(s)
}
fn host(s: &str) -> Result<(), String> {
    text(s, 256)?;
    if s.is_empty()
        || s.starts_with(['-', '.'])
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
    {
        return Err("Invalid expected server hostname".into());
    }
    Ok(())
}
fn oid(s: &str) -> Result<(), String> {
    if !matches!(s.len(), 40 | 64) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid stored commit ID".into());
    }
    Ok(())
}
