//! Capture and materialize portable public profile policy.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicAuthorRule {
    pub host: String,
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicProfile {
    pub source_id: u64,
    pub name: String,
    pub author_name: String,
    pub author_email: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_hosts: Vec<PublicAuthorRule>,
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
            author_hosts: p
                .author_hosts
                .iter()
                .map(|rule| PublicAuthorRule {
                    host: rule.host.clone(),
                    name: rule.name.clone(),
                    email: rule.email.clone(),
                })
                .collect(),
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
            author_hosts: self
                .author_hosts
                .iter()
                .map(|rule| crate::settings::profiles::HostAuthor {
                    host: rule.host.clone(),
                    name: rule.name.clone(),
                    email: rule.email.clone(),
                })
                .collect(),
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
