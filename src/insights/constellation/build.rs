//! Construct graph nodes and relationships from a captured snapshot.
use super::*;
use std::collections::BTreeMap;

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
