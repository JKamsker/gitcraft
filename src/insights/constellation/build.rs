//! Construct graph nodes and relationships from a captured snapshot.
use super::*;
use std::collections::BTreeMap;
mod forge_nodes;
mod local;

/// The constellation of `s`, with what `forge` says, at `now`.
pub fn build(s: &Snapshot, forge: &Forge, now: i64) -> Model {
    let mut m = Model::default();
    local::branches(&mut m, s, forge);
    local::worktrees(&mut m, s);
    local::tasks(&mut m, s, now);
    forge_nodes::pull_requests(&mut m, forge);
    forge_nodes::builds(&mut m, forge);
    branch_relationships(&mut m, s);

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

fn branch_relationships(m: &mut Model, s: &Snapshot) {
    let target = s.target.as_ref().map(|t| t.name.as_str());
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
}
