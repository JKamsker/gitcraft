//! Cleaning up the worktrees of merged branches (#213), on the lifecycle rules of #192: what a
//! worktree holds (changed, untracked and ignored files, its footprint) and whether removing
//! it could lose anything, read again just before it is removed. Untracked text files can be
//! kept first in a verified ZIP archive ([`archive`]). The branch itself is never deleted
//! here. Local repositories only: nothing runs on another machine.

pub(crate) mod archive;
mod execute;
mod files;
mod inspect;

use std::path::PathBuf;

pub(crate) use execute::{Outcome, remove};
pub(crate) use inspect::inspect;

/// How the branch's work is known to be in its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Proof {
    /// Its tip is reachable from the base ref.
    Ancestor,
    /// Every commit of it has an equivalent in the base (`git cherry`).
    PatchEquivalent,
    /// A pull request into the base proposed it at its tip or later, and was merged.
    PullRequest(u64),
}

/// Why a worktree is offered: its branch was found merged, at `tip`, into `base_ref`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Evidence {
    /// The full ref checked against (`refs/remotes/origin/dev`); the base branch's name for a
    /// pull request.
    pub base_ref: String,
    pub proof: Proof,
    /// The branch's commit when it was found merged: a branch that moved since is not.
    pub tip: String,
    /// What the preview says ("merged into origin/dev").
    pub describe: String,
}

/// A worktree to look at and maybe remove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub path: PathBuf,
    pub branch: String,
    pub evidence: Evidence,
    /// Why the app knows it is in use (an agent task runs there), if it does.
    pub in_use: Option<String>,
}

/// Files and bytes; `complete` is false when the count stopped at its limits (shown as
/// "at least").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Footprint {
    pub files: u64,
    pub bytes: u64,
    pub complete: bool,
}

/// What an untracked file is, for whether it can be archived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Text, with its SHA-256: archivable.
    Text(String),
    Binary,
    TooLarge,
    /// A symbolic link, or a file reached through one: never followed.
    Link,
    /// Not a plain file, unreadable, or a name that is not UTF-8.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Untracked {
    /// Relative to the worktree, `/` between names.
    pub path: String,
    pub size: u64,
    pub kind: Kind,
}

impl Footprint {
    /// "1.2 MB", or "≥ 1.2 MB" when counting stopped at its limits.
    pub fn describe(self) -> String {
        files::describe(self)
    }
}

impl Untracked {
    pub fn archivable(&self) -> bool {
        matches!(self.kind, Kind::Text(_))
    }
}

/// Why a worktree is not removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Blocker {
    /// The repository's main worktree.
    Primary,
    /// Git no longer lists it, or its folder is gone.
    Missing,
    Bare,
    Locked(String),
    /// It no longer has the branch checked out (another one, or a detached HEAD).
    BranchChanged,
    /// Its HEAD is not the branch's tip, or the tip is not the one found merged.
    Moved,
    /// The merge evidence does not hold any more.
    NotMerged,
    /// Tracked files with changes (staged, unstaged or conflicted).
    Changed(usize),
    /// A merge, rebase, cherry-pick, revert or bisect is in progress there.
    Operation,
    /// Git is working in it (its index is locked), or the app uses it.
    InUse(String),
    /// Untracked files, and archiving was not chosen.
    Untracked(usize),
    /// Untracked files that can't be archived (binary, too large, links, unreadable).
    NotArchivable(usize),
    /// Commits that are neither in the base nor on any remote.
    Unpublished(usize),
    /// On another machine: not supported.
    Remote,
}

impl Blocker {
    /// What the preview says keeps the worktree.
    pub fn describe(&self) -> String {
        let count = |id: &str, n: usize| crate::i18n::message(id, &[("count", n.into())]);
        match self {
            Blocker::Primary => crate::i18n::text("worktree-cleanup-blocker-primary").to_owned(),
            Blocker::Missing => crate::i18n::text("worktree-cleanup-blocker-missing").to_owned(),
            Blocker::Bare => crate::i18n::text("worktree-cleanup-blocker-bare").to_owned(),
            Blocker::Locked(reason) => crate::i18n::message(
                "worktree-cleanup-blocker-locked",
                &[("reason", reason.clone().into())],
            ),
            Blocker::BranchChanged => {
                crate::i18n::text("worktree-cleanup-blocker-branch-changed").to_owned()
            }
            Blocker::Moved => crate::i18n::text("worktree-cleanup-blocker-moved").to_owned(),
            Blocker::NotMerged => {
                crate::i18n::text("worktree-cleanup-blocker-not-merged").to_owned()
            }
            Blocker::Changed(n) => count("worktree-cleanup-blocker-changed", *n),
            Blocker::Operation => {
                crate::i18n::text("worktree-cleanup-blocker-operation").to_owned()
            }
            Blocker::InUse(why) => crate::i18n::message(
                "worktree-cleanup-blocker-in-use",
                &[("why", why.clone().into())],
            ),
            Blocker::Untracked(n) => count("worktree-cleanup-blocker-untracked", *n),
            Blocker::NotArchivable(n) => count("worktree-cleanup-blocker-not-archivable", *n),
            Blocker::Unpublished(n) => count("worktree-cleanup-blocker-unpublished", *n),
            Blocker::Remote => crate::i18n::text("worktree-cleanup-blocker-remote").to_owned(),
        }
    }
}

/// What a worktree holds, read just now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Inspection {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
    /// Tracked files with changes.
    pub changed: Vec<String>,
    pub untracked: Vec<Untracked>,
    /// Ignored files and folders, as git lists them, and what they hold.
    pub ignored: Vec<String>,
    pub ignored_size: Footprint,
    /// The whole folder: what removing it gives back.
    pub footprint: Footprint,
    /// Blockers that hold whatever is chosen about archiving.
    pub blockers: Vec<Blocker>,
    /// Over HEAD, the branch's tip and everything tracked or untracked: a change between the
    /// preview and the removal makes them differ.
    pub fingerprint: String,
}

impl Inspection {
    /// Everything that keeps it, given whether untracked text files are archived first.
    pub fn blockers_with(&self, archive: bool) -> Vec<Blocker> {
        let mut out = self.blockers.clone();
        let excluded = self.untracked.iter().filter(|u| !u.archivable()).count();
        if !archive && !self.untracked.is_empty() {
            out.push(Blocker::Untracked(self.untracked.len()));
        } else if archive && excluded > 0 {
            out.push(Blocker::NotArchivable(excluded));
        }
        out
    }
}

#[cfg(test)]
mod tests;
