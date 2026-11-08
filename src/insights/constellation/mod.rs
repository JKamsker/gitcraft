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

use std::collections::{BTreeMap, HashMap, HashSet};

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
/// branch `target` contains), and how the builds of its tip went.
pub fn branch_flags(
    b: &BranchIn,
    target: Option<&str>,
    merged: Option<&HashSet<String>>,
    builds: Option<&HashMap<String, CheckState>>,
) -> Vec<(Flag, Evidence)> {
    let mut flags = Vec::new();
    if b.upstream.is_empty() || b.gone || b.ahead > 0 {
        flags.push((Flag::Unpublished, Evidence::Observed));
    }
    if target != Some(b.name.as_str()) && merged.is_some_and(|m| m.contains(&b.name)) {
        flags.push((Flag::Merged, Evidence::Observed));
    }
    if let Some(flag) = builds
        .and_then(|m| m.get(&b.tip))
        .and_then(|s| build_flag(*s))
    {
        flags.push((flag, Evidence::Observed));
    }
    flags
}

/// The flag of a build state (none for skipped ones).
pub fn build_flag(state: CheckState) -> Option<Flag> {
    match state {
        CheckState::Passed => Some(Flag::Passed),
        CheckState::Failed => Some(Flag::Failed),
        CheckState::Running => Some(Flag::Running),
        CheckState::Skipped => None,
    }
}

/// Whether task `t`'s agent looks at work at `now`: active or idle from its last checkpoint
/// turn (inferred: gitcraft doesn't record whether the agent's process runs), nothing when no
/// turn was saved.
pub fn task_activity(t: &TaskIn, now: i64) -> Option<(Flag, Evidence)> {
    let last = t.last_turn?;
    let flag = if now - last <= ACTIVE_WITHIN {
        Flag::Active
    } else {
        Flag::Idle
    };
    Some((flag, Evidence::Inferred))
}

/// The flags of worktree `w`: dirty, idle (clean), missing, locked.
pub fn worktree_flags(w: &WorktreeIn) -> Vec<(Flag, Evidence)> {
    let mut flags = Vec::new();
    if w.prunable.is_some() {
        flags.push((Flag::Missing, Evidence::Observed));
    } else {
        match w.changes {
            Some(0) => flags.push((Flag::Idle, Evidence::Observed)),
            Some(_) => flags.push((Flag::Dirty, Evidence::Observed)),
            None => {}
        }
    }
    if w.locked.is_some() {
        flags.push((Flag::Locked, Evidence::Observed));
    }
    flags
}

fn same_path(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// The constellation of `s`, with what `forge` says, at `now`.
pub fn build(s: &Snapshot, forge: &Forge, now: i64) -> Model {
    let mut m = Model::default();
    let target = s.target.as_ref().map(|t| t.name.as_str());
    let builds = forge.builds.as_ref();

    // Branches first: those something else joins come first, so the columns line up.
    let mut order: Vec<&BranchIn> = s.branches.iter().collect();
    let joined = |b: &BranchIn| {
        s.worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some(&b.name))
            || s.tasks.iter().any(|t| t.branch == b.name)
            || forge
                .prs
                .as_ref()
                .is_some_and(|prs| prs.iter().any(|p| p.branch == b.name && !p.from_fork))
    };
    order.sort_by_key(|b| (!joined(b), Some(b.name.as_str()) != target));
    for b in order {
        let mut detail = vec![format!("tip {}", short(&b.tip))];
        if b.upstream.is_empty() {
            detail.push("no upstream".to_owned());
        } else if b.gone {
            detail.push(format!("upstream {} deleted on the remote", b.upstream));
        } else {
            detail.push(format!(
                "upstream {}: {} ahead, {} behind",
                b.upstream, b.ahead, b.behind
            ));
        }
        if Some(b.name.as_str()) == target {
            detail.push("the default branch".to_owned());
        } else if s.merged.is_none() {
            detail.push("merged: not known (no default branch found)".to_owned());
        }
        if let Some(parent) = s.stack_parents.get(&b.name) {
            detail.push(format!("stacked on {parent} (recorded)"));
        }
        m.nodes.push(Node {
            kind: Kind::Branch,
            key: b.name.clone(),
            label: b.name.clone(),
            flags: branch_flags(b, target, s.merged.as_ref(), builds),
            detail,
            commit: Some(b.tip.clone()),
        });
    }

    for w in &s.worktrees {
        let mut detail = Vec::new();
        match &w.branch {
            Some(branch) => detail.push(format!("on {branch}")),
            None => detail.push(format!("detached at {}", short(&w.head))),
        }
        match (w.prunable.as_ref(), w.changes) {
            (Some(why), _) => detail.push(format!("folder missing: {why}")),
            (None, Some(0)) => detail.push("no uncommitted changes".to_owned()),
            (None, Some(n)) => detail.push(format!("{n} files with uncommitted changes")),
            (None, None) => detail.push("uncommitted changes not counted".to_owned()),
        }
        if w.main {
            detail.push("the main worktree".to_owned());
        }
        if w.current {
            detail.push("shown in this tab".to_owned());
        }
        if let Some(reason) = &w.locked {
            detail.push(if reason.is_empty() {
                "locked".to_owned()
            } else {
                format!("locked: {reason}")
            });
        }
        let i = m.nodes.len();
        m.nodes.push(Node {
            kind: Kind::Worktree,
            key: w.path.clone(),
            label: folder_name(&w.path),
            flags: worktree_flags(w),
            detail,
            commit: (!w.head.is_empty()).then(|| w.head.clone()),
        });
        if let Some(b) = w.branch.as_ref().and_then(|b| m.find(Kind::Branch, b)) {
            m.edges.push(Edge {
                from: b,
                to: i,
                link: Link::CheckedOut,
                evidence: Evidence::Observed,
                note: "git worktree list".to_owned(),
            });
        }
    }

    for t in &s.tasks {
        let worktree = s
            .worktrees
            .iter()
            .find(|w| same_path(&w.path, &t.path) && w.prunable.is_none());
        let mut flags = Vec::new();
        let mut detail = vec![format!("{} on {}", t.agent, t.branch)];
        if !t.prompt.trim().is_empty() {
            detail.push(format!(
                "task: {}",
                t.prompt.lines().next().unwrap_or_default()
            ));
        }
        if t.started > 0 {
            detail.push(format!(
                "started {}",
                crate::git::relative_time(t.started, now)
            ));
        }
        match task_activity(t, now) {
            Some(activity) => {
                flags.push(activity);
                let when = t
                    .last_turn
                    .map(|at| crate::git::relative_time(at, now))
                    .unwrap_or_default();
                detail.push(format!(
                    "{} checkpoint turns, the last {when}; whether the agent still runs is \
                     not recorded",
                    t.turns
                ));
            }
            None => detail.push(
                "no checkpoint turns saved: whether its agent is at work is not known".to_owned(),
            ),
        }
        if worktree.is_none() {
            flags.push((Flag::Missing, Evidence::Observed));
            detail.push("its worktree is gone".to_owned());
        }
        let i = m.nodes.len();
        m.nodes.push(Node {
            kind: Kind::Task,
            key: t.path.clone(),
            label: format!("{} · {}", t.agent, folder_name(&t.path)),
            flags,
            detail,
            commit: None,
        });
        if let Some(w) = worktree.and_then(|w| m.find(Kind::Worktree, &w.path)) {
            m.edges.push(Edge {
                from: w,
                to: i,
                link: Link::TaskWorktree,
                evidence: Evidence::Observed,
                note: "recorded when the task started".to_owned(),
            });
        }
        if let Some(b) = m.find(Kind::Branch, &t.branch) {
            m.edges.push(Edge {
                from: b,
                to: i,
                link: Link::TaskBranch,
                evidence: Evidence::Observed,
                note: "recorded when the task started".to_owned(),
            });
        }
    }

    match &forge.prs {
        Some(prs) => {
            for p in prs {
                let Some(b) = m.find(Kind::Branch, &p.branch).filter(|_| !p.from_fork) else {
                    continue;
                };
                let tip = m.nodes[b].commit.clone().unwrap_or_default();
                let mut detail = vec![p.title.clone(), format!("{} into {}", p.branch, p.base)];
                if p.sha != tip {
                    detail.push(format!(
                        "its head {} is not the local tip {}",
                        short(&p.sha),
                        short(&tip)
                    ));
                }
                let flags = if p.draft {
                    vec![(Flag::Draft, Evidence::Observed)]
                } else {
                    Vec::new()
                };
                let i = m.nodes.len();
                m.nodes.push(Node {
                    kind: Kind::Pr,
                    key: p.number.to_string(),
                    label: format!("#{}", p.number),
                    flags,
                    detail,
                    commit: Some(p.sha.clone()),
                });
                m.edges.push(Edge {
                    from: b,
                    to: i,
                    link: Link::Proposes,
                    evidence: Evidence::Observed,
                    note: "the pull request's head branch".to_owned(),
                });
            }
        }
        None => m
            .notes
            .push("Pull requests: not read (no forge connected, or not loaded yet)".to_owned()),
    }

    match builds {
        Some(builds) => {
            // The commits built: branch tips and pull request heads, each once.
            let sources: Vec<(usize, String)> = m
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n.kind, Kind::Branch | Kind::Pr))
                .filter_map(|(i, n)| n.commit.clone().map(|c| (i, c)))
                .collect();
            for (from, sha) in sources {
                let Some(&state) = builds.get(&sha) else {
                    continue;
                };
                let i = match m.find(Kind::Build, &sha) {
                    Some(i) => i,
                    None => {
                        m.nodes.push(Node {
                            kind: Kind::Build,
                            key: sha.clone(),
                            label: format!("{} {}", short(&sha), state_word(state)),
                            flags: build_flag(state)
                                .map(|f| vec![(f, Evidence::Observed)])
                                .unwrap_or_default(),
                            detail: vec![format!(
                                "builds of {}: {}",
                                short(&sha),
                                state_word(state)
                            )],
                            commit: Some(sha.clone()),
                        });
                        m.nodes.len() - 1
                    }
                };
                m.edges.push(Edge {
                    from,
                    to: i,
                    link: Link::Built,
                    evidence: Evidence::Observed,
                    note: "the forge's checks of that commit".to_owned(),
                });
            }
            m.notes.push(
                "Commits without builds have none the forge reported: not known, not failing"
                    .to_owned(),
            );
        }
        None => m
            .notes
            .push("Builds: not read (no forge with CI connected, or not loaded yet)".to_owned()),
    }

    // Branch to branch: a recorded stack parent, else the same fork point (inferred).
    for (child, parent) in &s.stack_parents {
        if let (Some(c), Some(p)) = (m.find(Kind::Branch, child), m.find(Kind::Branch, parent)) {
            m.edges.push(Edge {
                from: c,
                to: p,
                link: Link::StackParent,
                evidence: Evidence::Observed,
                note: "recorded with “This branch is based on…”".to_owned(),
            });
        }
    }
    let mut by_base: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, n) in m.nodes.iter().enumerate() {
        if n.kind != Kind::Branch
            || Some(n.key.as_str()) == target
            || s.stack_parents.contains_key(&n.key)
            || n.flags.iter().any(|(f, _)| *f == Flag::Merged)
        {
            continue;
        }
        if let Some(base) = s.bases.get(&n.key) {
            by_base.entry(base.as_str()).or_default().push(i);
        }
    }
    for (base, group) in by_base {
        // A chain, not every pair: the same is said with fewer lines.
        for pair in group.windows(2) {
            m.edges.push(Edge {
                from: pair[0],
                to: pair[1],
                link: Link::SameBase,
                evidence: Evidence::Inferred,
                note: format!(
                    "both fork from {} at {}",
                    target.unwrap_or("the default branch"),
                    short(base)
                ),
            });
        }
    }

    if let Some(e) = &s.tasks_error {
        m.notes.push(format!("Agent tasks: couldn't be read ({e})"));
    }
    if s.target.is_none() {
        m.notes.push(
            "No default branch found (origin/HEAD, main, master): merged and same-base not known"
                .to_owned(),
        );
    }
    m.notes.push(
        "Conflicting edits between branches: not worked out here; the Conflict radar merges \
         them in memory"
            .to_owned(),
    );
    m.notes.push("Processes and ports: not measured".to_owned());
    m
}

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
