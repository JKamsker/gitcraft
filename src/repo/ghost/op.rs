//! The operations a ghost run previews, and what each needs to know of the repository.

use crate::git::{Branch, Refs};
use crate::repo::pull::PullMode;

/// An operation the graph can show the result of before it is done.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GhostOp {
    Timeline {
        state: crate::repo::timeline::RefState,
        label: String,
    },
    /// `git rebase <onto>` of the checked-out branch; `onto` a full ref name, `label` its short
    /// one.
    Rebase { onto: String, label: String },
    /// `git reset --hard <to>` of the checked-out branch (or of a detached HEAD).
    ResetHard { to: String },
    /// The toolbar's Pull of the checked-out branch, as `mode` does it.
    Pull(PullMode),
    /// `git push --force-with-lease` of the local branch `branch` to the branch it tracks.
    ForcePush { branch: String },
    /// Deleting the local branch `branch`, and the one it tracks on its remote with `remote`.
    DeleteBranch { branch: String, remote: bool },
    /// Dropping the commit `hash` from the checked-out branch.
    DropCommit { hash: String },
}

impl GhostOp {
    /// What the strip calls it.
    pub(crate) fn title(&self, refs: &Refs) -> String {
        let current = refs.current.as_deref().unwrap_or("HEAD");
        match self {
            GhostOp::Timeline { label, .. } => crate::i18n::message(
                "repo-workflows-ghost-title-20596011",
                &[("label", (label).clone().into())],
            ),
            GhostOp::Rebase { label, .. } => crate::i18n::message(
                "repo-workflows-ghost-title-2d281f6b",
                &[
                    ("current", (current).clone().into()),
                    ("label", (label).clone().into()),
                ],
            ),
            GhostOp::ResetHard { to } => crate::i18n::message(
                "repo-workflows-ghost-title-ae583650",
                &[
                    ("current", (current).clone().into()),
                    ("short-to-1", (short(to)).clone().into()),
                ],
            ),
            GhostOp::Pull(mode) => crate::i18n::message(
                "repo-workflows-ghost-title-baaedb85",
                &[
                    ("mode-label-1", (mode.label()).clone().into()),
                    ("current", (current).clone().into()),
                ],
            ),
            GhostOp::ForcePush { branch } => crate::i18n::message(
                "repo-workflows-ghost-title-b193a29c",
                &[("branch", (branch).clone().into())],
            ),
            GhostOp::DeleteBranch {
                branch,
                remote: false,
            } => crate::i18n::message(
                "repo-workflows-ghost-title-62fc0c1a",
                &[("branch", (branch).clone().into())],
            ),
            GhostOp::DeleteBranch { branch, .. } => crate::i18n::message(
                "repo-workflows-ghost-title-c58e3395",
                &[("branch", (branch).clone().into())],
            ),
            GhostOp::DropCommit { hash } => crate::i18n::message(
                "repo-workflows-ghost-title-fc4198eb",
                &[("short-hash-1", (short(hash)).clone().into())],
            ),
        }
    }
}

pub(super) fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// What the background work gets of the tab: the refs as read, the remotes, and whether the
/// working tree has changes.
#[derive(Clone, Default)]
pub(crate) struct Facts {
    pub(crate) refs: Refs,
    /// The remotes' names.
    pub(crate) remotes: Vec<String>,
    /// Tracked files have uncommitted changes (untracked files stop no rebase).
    pub(crate) dirty: bool,
}

/// A local branch's upstream: its remote, the branch's name there, and the remote-tracking
/// branch here.
pub(crate) struct Upstream<'a> {
    pub(crate) remote: &'a str,
    pub(crate) name: &'a str,
    pub(crate) tracking: &'a Branch,
}

/// Whether `work` has changes to tracked files, which a rebase refuses to start with.
pub(crate) fn tracked_changes(work: &crate::git::WorkStatus) -> bool {
    !work.staged.is_empty()
        || !work.conflicted.is_empty()
        || work
            .unstaged
            .iter()
            .any(|f| f.area != crate::git::Area::Untracked)
}

impl Facts {
    pub(crate) fn local(&self, name: &str) -> Option<&Branch> {
        self.refs.local.iter().find(|b| b.name == name)
    }

    /// The checked-out branch.
    pub(crate) fn current(&self) -> Result<&Branch, String> {
        self.refs
            .current
            .as_deref()
            .and_then(|c| self.local(c))
            .ok_or_else(|| crate::i18n::text("repo-workflows-ghost-current-7776bacf").to_owned())
    }

    /// Where `b` pushes and pulls, when it tracks a branch that is still there.
    pub(crate) fn upstream<'a>(&'a self, b: &'a Branch) -> Option<Upstream<'a>> {
        if b.upstream.is_empty() || b.upstream_gone() {
            return None;
        }
        let names: Vec<&str> = self.remotes.iter().map(String::as_str).collect();
        let remote = crate::repo::sidebar::remote_of(&b.upstream, &names)?;
        let name = crate::repo::sidebar::remote_short(&b.upstream, remote)?;
        let tracking = self
            .refs
            .remotes
            .iter()
            .flat_map(|(_, list)| list)
            .find(|r| r.name == b.upstream)?;
        let remote = self.remotes.iter().find(|r| r.as_str() == remote)?;
        Some(Upstream {
            remote,
            name,
            tracking,
        })
    }
}
