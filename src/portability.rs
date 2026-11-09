//! One versioned, allowlisted setup file. Credentials, executable commands, raw Git config,
//! auth folders, private/signing key values and old machine paths never become active config.
mod config;
mod display;
pub(crate) use display::message as display_message;
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
    /// Tea aliases are local labels; pin their actual public owner as well.
    #[serde(default)]
    pub account_user: Option<String>,
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
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]#[serde(default,deny_unknown_fields)]
pub(crate) struct Preferences{$(pub $field:$ty,)*}
impl From<&Settings> for Preferences{fn from(s:&Settings)->Self{Self{$($field:s.$field.clone(),)*}}}
impl Default for Preferences{fn default()->Self{Self::from(&Settings::default())}}
impl Preferences{pub fn apply(&self,s:&mut Settings){$(s.$field=self.$field.clone();)*}}
};}
preferences! {
ui_scale:f32,terminal_font:f32,refs_width:f32,auto_refs_width:bool,branch_names_keep_end:bool,graph_width:f32,compact_graph:bool,
author_width:f32,when_width:f32,date_width:f32,hash_width:f32,show_author:bool,show_when:bool,show_date:bool,show_hash:bool,
show_test_lane:bool,show_branches:bool,show_detail:bool,worktrees_in_same_tab:bool,tabs_in_title_bar:bool,history_map:crate::settings::HistoryMap,button_layout:crate::app_buttons::ButtonLayout,
files_as_tree:bool,diff_split:bool,diff_whole_file:bool,diff_beside_graph:bool,diff_algorithms:std::collections::BTreeMap<String,crate::git::DiffAlgorithm>,
fold_generated:bool,fold_patterns:String,auto_fetch_minutes:u32,offline:bool,pause_on_metered:bool,pull_mode:crate::repo::pull::PullMode,
avatars:bool,notifications:bool,check_updates:bool,update_channel:crate::update::Channel,download_updates:bool,vscode_colors:bool,
theme_tweaks:crate::theme::Tweaks,keep_submodules:bool,stale_branch_days:u32,tips:bool,seen_hints:std::collections::BTreeSet<String>
}

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
                        account_user: None,
                    });
                }
            }
            if !p.connections.tea_login.is_empty() {
                setup.accounts.push(PublicAccountExpectation {
                    provider: Provider::Tea,
                    host: p.connections.tea_host.clone(),
                    user_or_alias: p.connections.tea_login.clone(),
                    account_user: None,
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
        let mut setup = self.setup.clone();
        setup.signing_key_reselection_required |=
            self.own_signing && (self.sign_commits || self.sign_tags);
        p.pending_setup = setup.pending().then_some(setup);
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
    #[serde(default)]
    pub config: std::collections::BTreeMap<String, String>,
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
    /// Resolve public alias ownership on the exporting machine. Never serialize login tokens.
    pub(crate) fn resolve_public_accounts(&mut self) -> Result<(), String> {
        for profile in &mut self.profiles {
            for account in &mut profile.setup.accounts {
                if account.provider == Provider::Tea && account.account_user.is_none() {
                    let (host,user)=crate::forge::selected_tea_metadata(&account.user_or_alias).ok_or("Cannot export the selected tea account: local public ownership is unavailable")?;
                    if !host.eq_ignore_ascii_case(&account.host) || user.is_empty() {
                        return Err("The selected tea alias has a different host or no public owner; check local profile setup before export".into());
                    }
                    account.account_user = Some(user);
                }
            }
        }
        Ok(())
    }
    /// SSH addresses cannot carry the old machine's alias/agent policy. Require a
    /// fresh supported explicit key even when the original profile used ambient SSH.
    pub(crate) fn require_repository_key_setup(&mut self) {
        let ids = self
            .repositories
            .iter()
            .filter(|r| {
                r.remotes.iter().any(|remote| {
                    std::iter::once(&remote.fetch)
                        .chain(&remote.push)
                        .any(|url| !url.starts_with("https://"))
                })
            })
            .flat_map(|r| r.profile_ids.iter().copied())
            .collect::<std::collections::BTreeSet<_>>();
        for profile in &mut self.profiles {
            if ids.contains(&profile.source_id) {
                profile.setup.ssh_key_reselection_required = true;
            }
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > FILE_LIMIT {
            return Err("Setup file exceeds 8 MiB".into());
        }
        let mut file: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid setup file: {e}"))?;
        file.require_repository_key_setup();
        file.validate()?;
        Ok(file)
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut file = self.clone();
        file.require_repository_key_setup();
        file.validate()?;
        let bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
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
        if self.preferences.fold_patterns.len() > 65536
            || self.preferences.fold_patterns.contains('\0')
        {
            return Err("Invalid fold patterns".into());
        }
        if self.preferences.diff_algorithms.len() > 256 || self.preferences.seen_hints.len() > 512 {
            return Err("Too many portable preference values".into());
        }
        for key in self
            .preferences
            .diff_algorithms
            .keys()
            .chain(self.preferences.seen_hints.iter())
        {
            text(key, 512)?;
        }
        if self.preferences.auto_fetch_minutes > 10080 || self.preferences.stale_branch_days > 36500
        {
            return Err("Invalid portable scheduling setting".into());
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
            let mut providers = std::collections::BTreeSet::new();
            for a in &p.setup.accounts {
                if !providers.insert(format!("{:?}", a.provider)) {
                    return Err("Duplicate provider expectation".into());
                }
                text(&a.user_or_alias, 256)?;
                host(&a.host)?;
                if a.user_or_alias.is_empty() {
                    return Err("Expected account is empty".into());
                }
                if let Some(user) = &a.account_user {
                    text(user, 256)?;
                    if user.is_empty() {
                        return Err("Expected account owner is empty".into());
                    }
                }
                if a.provider == Provider::Tea && a.account_user.is_none() {
                    return Err(
                        "A portable tea alias must include its expected public account owner"
                            .into(),
                    );
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
            if !names.insert(&r.name) {
                return Err("Duplicate repository destination name".into());
            }
            if r.profile_ids.iter().any(|id| !ids.contains(id)) {
                return Err("Repository references an unknown profile".into());
            }
            r.validate()?;
        }
        Ok(())
    }
}
impl Repository {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let r = self;
        config::validate(&r.config)?;
        leaf(&r.name)?;
        text(&r.previous_path, 4096)?;
        if r.profile_ids.len() > 64 {
            return Err("Too many profile associations".into());
        }
        if r.remotes.len() > 32
            || r.worktrees.len() > 64
            || r.hidden.len() > 2000
            || r.notices.len() > 64
        {
            return Err("Repository metadata exceeds limits".into());
        }
        let mut remote_names = std::collections::BTreeSet::new();
        for remote in &r.remotes {
            reference(&remote.name)?;
            if !remote_names.insert(&remote.name) {
                return Err("Duplicate remote name".into());
            }
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
fn windows_reserved(s: &str) -> bool {
    let stem = s.split('.').next().unwrap_or_default().to_ascii_lowercase();
    ["con", "prn", "aux", "nul"].contains(&stem.as_str())
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
        })
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
        || windows_reserved(s)
    {
        return Err("Choose a plain, unique folder name".into());
    }
    Ok(())
}
pub(crate) fn reference(s: &str) -> Result<(), String> {
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
