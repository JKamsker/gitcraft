//! Captured repository and forge inputs to the constellation.
use super::*;
use std::collections::BTreeMap;

/// A local branch, as `git for-each-ref` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchIn {
    pub name: String,
    pub tip: String,
    /// Its upstream (`origin/x`), empty when it has none.
    pub upstream: String,
    pub ahead: u32,
    pub behind: u32,
    /// Its upstream was deleted on the remote.
    pub gone: bool,
    /// When its tip was committed (seconds since the epoch).
    pub committed: i64,
}

/// A worktree, as `git worktree list` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeIn {
    /// Its folder, in the form the app gives paths (`ssh://host/…` on another machine).
    pub path: String,
    pub branch: Option<String>,
    pub head: String,
    /// Files with uncommitted changes; `None` when not counted (bare, folder missing…).
    pub changes: Option<usize>,
    /// Why git would prune it (its folder is gone), when it would.
    pub prunable: Option<String>,
    pub locked: Option<String>,
    pub main: bool,
    /// The one the tab shows.
    pub current: bool,
}

/// An agent task, as gitcraft recorded it when it started one, with what its checkpoint
/// turns say of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskIn {
    pub branch: String,
    /// Its worktree's folder, as recorded.
    pub path: String,
    /// The agent's name (Claude Code, Codex…).
    pub agent: String,
    pub prompt: String,
    pub started: i64,
    /// Checkpoint turns saved in its worktree, and when the last one was.
    pub turns: usize,
    pub last_turn: Option<i64>,
}

/// An open pull request, as the forge listed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrIn {
    pub number: u64,
    pub title: String,
    /// The branch it proposes.
    pub branch: String,
    pub base: String,
    /// Its head commit.
    pub sha: String,
    pub draft: bool,
    pub from_fork: bool,
}

/// The default branch the others are measured against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Target {
    /// As shown: `main`, or `origin/main` when there is no local one.
    pub name: String,
    /// As given to git: `refs/heads/main`, `refs/remotes/origin/main`.
    pub rev: String,
    pub tip: String,
}

/// What Git and the agent task list said, read in the background.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub branches: Vec<BranchIn>,
    pub worktrees: Vec<WorktreeIn>,
    pub tasks: Vec<TaskIn>,
    /// Why the task list couldn't be read, when it couldn't.
    pub tasks_error: Option<String>,
    /// Branch → the parent it is recorded as stacked on.
    pub stack_parents: BTreeMap<String, String>,
    pub target: Option<Target>,
    /// The branches whose tip the default branch contains; `None` without a default branch.
    pub merged: Option<HashSet<String>>,
    /// Branch → its merge-base with the default branch.
    pub bases: HashMap<String, String>,
    /// Every branch tip read, to tell once one moves.
    pub tips: Vec<(String, String)>,
    /// The repository is on another machine (WSL, SSH).
    pub remote: bool,
    pub provenance: Provenance,
}

/// What the forge said, as the tab last read it. `None` for either: not read (no forge, not
/// signed in, not loaded yet), which is not the same as none there.
#[derive(Clone, Debug, Default)]
pub struct Forge {
    pub prs: Option<Vec<PrIn>>,
    /// Commit → how its builds went.
    pub builds: Option<HashMap<String, CheckState>>,
}
