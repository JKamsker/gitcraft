//! Derive evidence and flags from captured branch, worktree and task state.
use super::*;

pub fn branch_flags(
    b: &BranchIn,
    target: Option<&Target>,
    merged: Option<&HashSet<String>>,
    builds: Option<&HashMap<String, CheckState>>,
) -> Vec<(Flag, Evidence)> {
    let mut flags = Vec::new();
    if b.upstream.is_empty() || b.gone || b.ahead > 0 {
        flags.push((Flag::Unpublished, Evidence::Observed));
    }
    let own_commits = target.is_none_or(|t| t.name != b.name && t.tip != b.tip);
    if own_commits && merged.is_some_and(|m| m.contains(&b.name)) {
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

pub(super) fn same_path(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}
