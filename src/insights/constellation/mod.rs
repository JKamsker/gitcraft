//! The worktree and agent constellation (#175 item 3): the local branches, the worktrees they
//! are checked out in, the coding-agent tasks gitcraft recorded, the pull requests proposing
//! the branches and the builds of their tips, joined into one graph.
//!
//! Every node is something Git, the agent task list or the forge says is there; nothing is
//! made up from a branch's name. What can only be worked out (an agent being at work from its
//! last checkpoint, two branches forking from the default branch at the same commit) is marked
//! inferred, and what isn't read at all is said so rather than shown as missing.

pub mod read;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use super::{Evidence, Provenance};
use crate::ci::CheckState;

/// Most local branches read; past it, the most recently committed ones (and those checked out,
/// in a task or the default branch) are kept and the view says it is partial.
pub const MAX_BRANCHES: usize = 40;
/// Most worktrees read.
pub const MAX_WORKTREES: usize = 40;
/// Most agent tasks whose checkpoint turns are read.
pub const MAX_TASKS: usize = 24;
/// A task whose worktree saved a checkpoint turn this recently (in seconds) is taken as active.
pub const ACTIVE_WITHIN: i64 = 15 * 60;

mod snapshot;
pub use snapshot::{BranchIn, Forge, PrIn, Snapshot, Target, TaskIn, WorktreeIn};

/// What a node stands for: its column, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Branch,
    Worktree,
    Task,
    Pr,
    Build,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Branch => "branch",
            Kind::Worktree => "worktree",
            Kind::Task => "agent task",
            Kind::Pr => "pull request",
            Kind::Build => "builds",
        }
    }
}

/// A state a node is in, each drawn with a letter of its own (never by colour alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Flag {
    /// An agent task whose worktree saved a checkpoint turn lately (inferred).
    Active,
    /// An agent task whose last checkpoint turn is older (inferred), or a clean worktree.
    Idle,
    /// A worktree with uncommitted changes.
    Dirty,
    /// A branch with no upstream, a deleted one, or commits its upstream hasn't.
    Unpublished,
    /// A branch whose tip the default branch contains.
    Merged,
    /// Builds that failed.
    Failed,
    /// Builds still running.
    Running,
    /// Builds that passed.
    Passed,
    /// A worktree whose folder is gone, or a task whose worktree is.
    Missing,
    Locked,
    /// A draft pull request.
    Draft,
}

impl Flag {
    /// The letter it is drawn with.
    pub fn glyph(self) -> &'static str {
        match self {
            Flag::Active => "A",
            Flag::Idle => "I",
            Flag::Dirty => "D",
            Flag::Unpublished => "U",
            Flag::Merged => "M",
            Flag::Failed => "F",
            Flag::Running => "R",
            Flag::Passed => "P",
            Flag::Missing => "X",
            Flag::Locked => "L",
            Flag::Draft => "d",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Flag::Active => "active",
            Flag::Idle => "idle",
            Flag::Dirty => "dirty",
            Flag::Unpublished => "unpublished",
            Flag::Merged => "merged",
            Flag::Failed => "failed",
            Flag::Running => "running",
            Flag::Passed => "passed",
            Flag::Missing => "missing",
            Flag::Locked => "locked",
            Flag::Draft => "draft",
        }
    }

    /// Every flag, in the legend's order.
    pub const ALL: [Flag; 11] = [
        Flag::Active,
        Flag::Idle,
        Flag::Dirty,
        Flag::Unpublished,
        Flag::Merged,
        Flag::Failed,
        Flag::Running,
        Flag::Passed,
        Flag::Missing,
        Flag::Locked,
        Flag::Draft,
    ];
}

/// A node of the constellation.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: Kind,
    /// What it is: a branch's name, a worktree's or task's folder, a pull request's number, a
    /// build's commit.
    pub key: String,
    pub label: String,
    /// Its states, each with how it is known.
    pub flags: Vec<(Flag, Evidence)>,
    /// Lines saying more of it, for its tooltip and details.
    pub detail: Vec<String>,
    /// The commit it is at: a branch's tip, a worktree's HEAD, a pull request's head, the
    /// commit built.
    pub commit: Option<String>,
}

/// How two nodes are joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Link {
    /// The branch is checked out in the worktree.
    CheckedOut,
    /// The task was started in the worktree.
    TaskWorktree,
    /// The task was started on the branch.
    TaskBranch,
    /// The pull request proposes the branch.
    Proposes,
    /// The builds are of the branch's tip or the pull request's head.
    Built,
    /// The branch is recorded as stacked on the other.
    StackParent,
    /// The two branches fork from the default branch at the same commit (inferred).
    SameBase,
}

impl Link {
    pub fn label(self) -> &'static str {
        match self {
            Link::CheckedOut => "checked out in",
            Link::TaskWorktree => "task in worktree",
            Link::TaskBranch => "task on branch",
            Link::Proposes => "proposed by",
            Link::Built => "built",
            Link::StackParent => "stacked on",
            Link::SameBase => "same base as",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub link: Link,
    pub evidence: Evidence,
    pub note: String,
}

/// The constellation: its nodes, how they are joined, and what isn't known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// What wasn't read or isn't measured, said plainly.
    pub notes: Vec<String>,
}

impl Model {
    /// The node of `kind` keyed so, if there is one.
    pub fn find(&self, kind: Kind, key: &str) -> Option<usize> {
        self.nodes
            .iter()
            .position(|n| n.kind == kind && n.key == key)
    }

    /// The edges touching node `i`, each with the node at its other end.
    pub fn links_of(&self, i: usize) -> Vec<(&Edge, usize)> {
        self.edges
            .iter()
            .filter_map(|e| {
                if e.from == i {
                    Some((e, e.to))
                } else if e.to == i {
                    Some((e, e.from))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// The flags of branch `b`: unpublished, merged (`merged` holds the branches the default
/// branch `target` contains; one at its very tip has no commits of its own to have been
/// merged), and how the builds of its tip went.
mod flags;
use flags::same_path;
pub use flags::{branch_flags, build_flag, task_activity, worktree_flags};

mod build;
pub use build::build;

/// A commit as the UI shows it: its first 8 characters.
pub fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// How a build went, in a word.
pub fn state_word(state: CheckState) -> &'static str {
    match state {
        CheckState::Passed => "passed",
        CheckState::Failed => "failed",
        CheckState::Running => "running",
        CheckState::Skipped => "skipped",
    }
}

/// The last part of a folder's path.
pub fn folder_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_owned()
}

/// The branches of `tips` whose tip moved or went since: what `current` says each name is at
/// now.
pub fn moved(tips: &[(String, String)], current: impl Fn(&str) -> Option<String>) -> Vec<String> {
    tips.iter()
        .filter(|(name, sha)| current(name).as_deref() != Some(sha.as_str()))
        .map(|(name, _)| name.clone())
        .collect()
}
