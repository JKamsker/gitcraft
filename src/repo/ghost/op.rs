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
            GhostOp::Timeline { label, .. } => format!("Undo tree: {label}"),
            GhostOp::Rebase { label, .. } => format!("Rebase {current} onto {label}"),
            GhostOp::ResetHard { to } => format!("Hard reset of {current} to {}", short(to)),
            GhostOp::Pull(mode) => format!("{} into {current}", mode.label()),
            GhostOp::ForcePush { branch } => format!("Force push of {branch}"),
            GhostOp::DeleteBranch {
                branch,
                remote: false,
            } => format!("Delete {branch}"),
            GhostOp::DeleteBranch { branch, .. } => {
                format!("Delete {branch} here and on its remote")
            }
            GhostOp::DropCommit { hash } => format!("Drop commit {}", short(hash)),
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
            .ok_or_else(|| "HEAD is detached: check out a branch first".to_owned())
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
