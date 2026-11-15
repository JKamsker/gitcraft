//! Forge identities and provider selection.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Forge {
    GitHub,
    GitLab,
    Forgejo,
    Gitea,
    Azure,
    Bitbucket,
}

impl Forge {
    pub const ALL: [Forge; 6] = [
        Forge::GitHub,
        Forge::GitLab,
        Forge::Forgejo,
        Forge::Gitea,
        Forge::Azure,
        Forge::Bitbucket,
    ];

    /// The module that does the work.
    pub fn provider(self) -> &'static dyn Provider {
        match self {
            Forge::GitHub => &github::GitHub,
            Forge::GitLab => &gitlab::GitLab,
            Forge::Forgejo => &gitea::FORGEJO,
            Forge::Gitea => &gitea::GITEA,
            Forge::Azure => &azure::Azure,
            Forge::Bitbucket => &bitbucket::Bitbucket,
        }
    }

    /// "GitHub", "Forgejo"…
    pub fn name(self) -> &'static str {
        self.provider().name()
    }

    /// "pull request" or "merge request".
    pub fn noun(self) -> &'static str {
        self.provider().noun()
    }

    /// Localized presentation; `noun()` retains its existing protocol identity.
    pub fn noun_label(self) -> &'static str {
        crate::i18n::text(match self {
            Forge::GitLab => "backend-noun-merge-request",
            _ => "backend-noun-pull-request",
        })
    }

    /// The heading of the sidebar's list.
    pub fn list_title(self) -> &'static str {
        match self.noun() {
            "merge request" => crate::i18n::text("backend-merge-requests-6e2149"),
            _ => crate::i18n::text("backend-pull-requests-334b28"),
        }
    }

    /// Whether gitcraft can show its pull requests in full (and review and merge them).
    pub fn has_pages(self) -> bool {
        self.provider().pages().is_some()
    }

    /// The name `gitcraft.forge` is set to in git's config to pick this forge.
    pub fn key(self) -> &'static str {
        match self {
            Forge::GitHub => "github",
            Forge::GitLab => "gitlab",
            Forge::Forgejo => "forgejo",
            Forge::Gitea => "gitea",
            Forge::Azure => "azure",
            Forge::Bitbucket => "bitbucket",
        }
    }

    pub fn from_key(key: &str) -> Option<Forge> {
        let key = key.trim().to_lowercase();
        Forge::ALL.into_iter().find(|forge| forge.key() == key)
    }
}
