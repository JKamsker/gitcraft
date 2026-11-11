//! Local Git and recorded task nodes, in deterministic column order.
use super::*;

pub(super) fn branches(m: &mut Model, s: &Snapshot, forge: &Forge) {
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
        } else if s.target.as_ref().is_some_and(|t| t.tip == b.tip) {
            detail.push("no commits of its own: at the default branch's tip".to_owned());
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
            flags: branch_flags(b, s.target.as_ref(), s.merged.as_ref(), builds),
            detail,
            commit: Some(b.tip.clone()),
        });
    }
}

pub(super) fn worktrees(m: &mut Model, s: &Snapshot) {
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
}

pub(super) fn tasks(m: &mut Model, s: &Snapshot, now: i64) {
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
}
