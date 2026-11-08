//! What a forge tells about a pull request beyond the pull request itself: review threads with
//! their state, the pending review of the person logged in, and what they may do.

use crate::pull_request::DiffSide;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadComment {
    pub id: String,
    pub author: String,
    pub at: i64,
    pub body: String,
    pub url: String,
    /// In the pending review of the person logged in: nobody else sees it yet.
    pub pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub path: String,
    pub line: Option<u64>,
    pub start_line: Option<u64>,
    pub side: DiffSide,
    pub outdated: bool,
    pub resolved: bool,
    pub can_resolve: bool,
    pub can_unresolve: bool,
    pub can_reply: bool,
    pub comments: Vec<ThreadComment>,
    /// It starts on the old version of the file and ends on the new one.
    pub starts_left: bool,
    /// It has more comments than were read.
    pub more_comments: bool,
}

/// Native providers declare supported review operations; existing forge behavior defaults on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewFeatures {
    pub pending: bool,
    pub request_changes: bool,
    pub approve: bool,
    pub apply_suggestions: bool,
}
impl Default for ReviewFeatures {
    fn default() -> Self {
        Self {
            pending: true,
            request_changes: true,
            approve: true,
            apply_suggestions: true,
        }
    }
}
/// The immutable initiating account/profile context; never a hash of a mutable global login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionBinding(crate::connections::Context);
pub(crate) fn connection_binding() -> ConnectionBinding {
    ConnectionBinding(crate::connections::current())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrExtras {
    pub pr_id: String,
    pub review_features: ReviewFeatures,
    pub native_account: Option<String>,
    /// Native merge policy is authoritative instead of GitHub-specific status strings.
    pub native_policy: bool,
    pub merge_blocker: Option<String>,
    pub connection_binding: Option<ConnectionBinding>,
    pub review_revision: Option<String>,
    pub viewer: Option<String>,
    pub permission: String,
    pub merge_allowed: bool,
    pub squash_allowed: bool,
    pub rebase_allowed: bool,
    pub delete_branch_on_merge: bool,
    /// `owner/name` of where the proposed branch lives; `None` when that repository is gone.
    pub head_repo: Option<String>,
    pub pending_review: Option<String>,
    /// The commit the threads' line numbers belong to.
    pub head_oid: String,
    /// The person logged in may merge past the repository's rules.
    pub can_bypass_rules: bool,
    pub threads: Vec<Thread>,
}

impl PrExtras {
    /// Whether the person logged in may push and merge here. Forges say it in their own words;
    /// their modules put it in GitHub's.
    pub fn can_write(&self) -> bool {
        matches!(self.permission.as_str(), "WRITE" | "MAINTAIN" | "ADMIN")
    }

    /// The comments waiting in the pending review.
    pub fn pending_comments(&self) -> impl Iterator<Item = (&Thread, &ThreadComment)> {
        self.threads.iter().flat_map(|thread| {
            thread
                .comments
                .iter()
                .filter(|comment| comment.pending)
                .map(move |comment| (thread, comment))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_some_permissions_may_write() {
        for (permission, may) in [
            ("READ", false),
            ("TRIAGE", false),
            ("WRITE", true),
            ("MAINTAIN", true),
            ("ADMIN", true),
        ] {
            let extras = PrExtras {
                permission: permission.to_owned(),
                ..PrExtras::default()
            };
            assert_eq!(extras.can_write(), may, "{permission}");
        }
    }
}
