//! The graph a ghost run draws: the rows shown now with the outcome laid over them. The
//! commits it would make go on top (they would be the newest), the refs move to where they
//! would point, and the commits it would leave on no ref stay, marked as going. Laid out as the
//! graph is, so its lanes are those the graph would have after it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::plan::Outcome;
use crate::git::Commit;
use crate::graph::{self, Graph};
use crate::repo::WIP_HASH;

/// What a row of the ghost graph is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    /// As it is now.
    #[default]
    Real,
    /// A commit the operation would make (or bring in).
    New,
    /// A commit it would leave on no ref.
    Going,
}

/// How a row of the ghost graph is drawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowGhost {
    pub(crate) kind: Kind,
    /// The lanes crossing the row that lead to or from a new commit: drawn as ghost lanes.
    pub(crate) through: Vec<usize>,
    /// The lanes coming into the row's dot from a new commit above.
    pub(crate) incoming: Vec<usize>,
    /// The lanes crossing the row from a commit it would leave on no ref: they go with it.
    pub(crate) going_through: Vec<usize>,
    /// The lanes coming into the row's dot from a commit above that goes.
    pub(crate) going_incoming: Vec<usize>,
}

/// The ghost graph, in the form the graph view draws (its rows, layout and index), with how
/// each row and pill is drawn.
#[derive(Default)]
pub(crate) struct GhostView {
    pub(crate) commits: Vec<Arc<Commit>>,
    pub(crate) graph: Graph,
    pub(crate) index: HashMap<String, usize>,
    pub(crate) rows: Vec<RowGhost>,
    /// The refs (full names) drawn where the operation would put them.
    pub(crate) moved: HashSet<String>,
    /// The refs (full names) it would delete, drawn struck out where they are.
    pub(crate) gone: HashSet<String>,
    /// How many rows it adds above the first of the rows shown now.
    pub(crate) added: usize,
}

impl GhostView {
    /// How row `i` is drawn.
    pub(crate) fn row(&self, i: usize) -> Option<&RowGhost> {
        self.rows.get(i)
    }
}

/// The decorations `git log --format=%D` gives the ref `full` (short and, with
/// `log.decorate=full`, full).
fn decorations(full: &str) -> Vec<String> {
    if let Some(name) = full.strip_prefix("refs/heads/") {
        vec![
            name.to_owned(),
            full.to_owned(),
            format!("HEAD -> {name}"),
            format!("HEAD -> {full}"),
        ]
    } else if let Some(name) = full.strip_prefix("refs/remotes/") {
        vec![name.to_owned(), full.to_owned()]
    } else if let Some(name) = full.strip_prefix("refs/tags/") {
        vec![format!("tag: {name}"), format!("tag: {full}")]
    } else {
        vec![full.to_owned()]
    }
}

/// The decoration `full` gets where it moves to; `head`: HEAD is on it.
fn decoration_at(full: &str, head: bool) -> String {
    match full.strip_prefix("refs/heads/") {
        Some(name) if head => format!("HEAD -> {name}"),
        Some(name) => name.to_owned(),
        None => match full.strip_prefix("refs/remotes/") {
            Some(name) => name.to_owned(),
            None => match full.strip_prefix("refs/tags/") {
                Some(name) => format!("tag: {name}"),
                None => full.to_owned(),
            },
        },
    }
}

/// The ghost graph of `outcome` over `rows` (the graph's rows now, the changes row first when
/// there is one). `head_ref`: the branch HEAD is on (`refs/heads/x`), `None` when detached.
pub(crate) fn build(rows: &[Arc<Commit>], outcome: &Outcome, head_ref: Option<&str>) -> GhostView {
    let mut view = GhostView::default();
    // The decorations each commit loses and gains.
    let mut removed: HashMap<&str, Vec<String>> = HashMap::new();
    let mut added: HashMap<&str, Vec<String>> = HashMap::new();
    let mut head_to = None;
    for m in &outcome.moves {
        let forms = decorations(&m.full);
        let on_head = head_ref == Some(m.full.as_str()) || m.full == "HEAD";
        match (&m.from, &m.to) {
            (_, None) => {
                view.gone.insert(m.full.clone());
            }
            (from, Some(to)) => {
                if let Some(from) = from {
                    removed.entry(from.as_str()).or_default().extend(forms);
                }
                added
                    .entry(to.as_str())
                    .or_default()
                    .push(decoration_at(&m.full, on_head && m.full != "HEAD"));
                view.moved.insert(m.full.clone());
                if on_head {
                    head_to = Some(to.clone());
                }
            }
        }
    }
    let edit = |c: &Arc<Commit>| -> Arc<Commit> {
        let (gone, new) = (removed.get(c.hash.as_str()), added.get(c.hash.as_str()));
        let wip_moves = c.hash == WIP_HASH && head_to.is_some();
        if gone.is_none() && new.is_none() && !wip_moves {
            return Arc::clone(c);
        }
        let mut c = (**c).clone();
        if let Some(gone) = gone {
            c.refs.retain(|r| !gone.contains(r));
        }
        if let Some(new) = new {
            c.refs.extend(new.iter().cloned());
        }
        if wip_moves && let Some(to) = &head_to {
            c.parents = vec![to.clone()];
        }
        Arc::new(c)
    };
    let created: HashSet<&str> = outcome.created.iter().map(|c| c.hash.as_str()).collect();
    let wip = rows.first().filter(|c| c.hash == WIP_HASH);
    let mut commits = Vec::with_capacity(rows.len() + outcome.created.len());
    let mut kinds = Vec::with_capacity(commits.capacity());
    if let Some(wip) = wip {
        commits.push(edit(wip));
        // A hard reset throws the uncommitted changes away.
        kinds.push(if outcome.discards_work {
            Kind::Going
        } else {
            Kind::Real
        });
    }
    for c in &outcome.created {
        commits.push(edit(&Arc::new(c.clone())));
        kinds.push(Kind::New);
    }
    for c in rows.iter().skip(usize::from(wip.is_some())) {
        // Already drawn (fetched commits the graph has through another branch).
        if created.contains(c.hash.as_str()) {
            continue;
        }
        commits.push(edit(c));
        kinds.push(if outcome.vanish.contains(&c.hash) {
            Kind::Going
        } else {
            Kind::Real
        });
    }
    view.added = commits.len().saturating_sub(rows.len());
    view.graph = graph::layout(&commits);
    view.rows = ghost_lanes(&view.graph, &kinds);
    view.index = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    view.commits = commits;
    view
}

/// Which lanes of each row lead to or from a new commit, walking the rows as the layout did:
/// a lane a new commit opens towards its parent is a ghost lane down to that parent's dot.
fn ghost_lanes(graph: &Graph, kinds: &[Kind]) -> Vec<RowGhost> {
    let mut ghost: Vec<bool> = vec![false; graph.width + 1];
    let mut going: Vec<bool> = vec![false; graph.width + 1];
    let mut out = Vec::with_capacity(graph.rows.len());
    for (row, &kind) in graph.rows.iter().zip(kinds) {
        let is = |lanes: &[usize], ghost: &[bool]| -> Vec<usize> {
            lanes
                .iter()
                .copied()
                .filter(|&l| ghost.get(l).copied().unwrap_or(false))
                .collect()
        };
        let through = is(&row.through, &ghost);
        let incoming = is(&row.incoming, &ghost);
        let going_through = is(&row.through, &going);
        let going_incoming = is(&row.incoming, &going);
        for &l in row.incoming.iter() {
            for lanes in [&mut ghost, &mut going] {
                if let Some(g) = lanes.get_mut(l) {
                    *g = false;
                }
            }
        }
        let (new, gone) = (kind == Kind::New, kind == Kind::Going);
        for (n, &l) in row.outgoing.iter().enumerate() {
            if l >= ghost.len() {
                ghost.resize(l + 1, false);
                going.resize(l + 1, false);
            }
            // A lane already running down (another child's) stays real if that one is.
            let shared = n > 0 && row.through.contains(&l);
            ghost[l] = if shared { ghost[l] && new } else { new };
            going[l] = if shared { going[l] && gone } else { gone };
        }
        out.push(RowGhost {
            kind,
            through,
            incoming,
            going_through,
            going_incoming,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::plan::{Outcome, RefMove};
    use super::*;

    fn c(hash: &str, parents: &[&str], refs: &[&str]) -> Arc<Commit> {
        Arc::new(Commit {
            hash: hash.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            summary: format!("commit {hash}"),
            refs: refs.iter().map(|r| (*r).to_owned()).collect(),
            ..Commit::default()
        })
    }

    fn moved(full: &str, from: &str, to: &str) -> RefMove {
        RefMove {
            full: full.to_owned(),
            from: Some(from.to_owned()),
            to: Some(to.to_owned()),
        }
    }

    /// main: m2 → m1 → base; feature (checked out): f2 → f1 → base.
    fn rows() -> Vec<Arc<Commit>> {
        vec![
            c("m2", &["m1"], &["main"]),
            c("f2", &["f1"], &["HEAD -> feature"]),
            c("m1", &["base"], &[]),
            c("f1", &["base"], &[]),
            c("base", &[], &[]),
        ]
    }

    /// A rebase of feature onto main: its copies on top, on main, with the pill; the
    /// originals going; the lane from the copies down to main a ghost lane.
    #[test]
    fn a_rebase_puts_its_copies_on_top_and_the_originals_going() {
        let outcome = Outcome {
            moves: vec![moved("refs/heads/feature", "f2", "f2'")],
            created: vec![
                (*c("f2'", &["f1'"], &[])).clone(),
                (*c("f1'", &["m2"], &[])).clone(),
            ],
            vanish: ["f2", "f1"].map(str::to_owned).into(),
            ..Outcome::default()
        };
        let view = build(&rows(), &outcome, Some("refs/heads/feature"));
        let hashes: Vec<&str> = view.commits.iter().map(|c| c.hash.as_str()).collect();
        assert_eq!(hashes, ["f2'", "f1'", "m2", "f2", "m1", "f1", "base"]);
        let kinds: Vec<Kind> = view.rows.iter().map(|r| r.kind).collect();
        use Kind::*;
        assert_eq!(kinds, [New, New, Real, Going, Real, Going, Real]);
        assert_eq!(view.commits[0].refs, ["HEAD -> feature"]);
        assert!(
            view.commits[3].refs.is_empty(),
            "{:?}",
            view.commits[3].refs
        );
        assert!(view.moved.contains("refs/heads/feature"));
        assert_eq!(view.added, 2);
        // The copies' lane comes into main's dot as a ghost lane; main's own lane is real.
        let main = &view.rows[2];
        assert_eq!(main.incoming, [view.graph.rows[2].col]);
        assert!(view.rows[3].through.is_empty());
        assert_eq!(view.index.get("m2"), Some(&2));
    }

    /// A hard reset moves the pill down to an existing commit, with nothing new: the commits
    /// above it on the branch go, and the changes row hangs from the new HEAD.
    #[test]
    fn a_reset_moves_the_pill_and_the_changes_row() {
        let mut list = vec![c(WIP_HASH, &["f2"], &[])];
        list.extend(rows());
        let outcome = Outcome {
            moves: vec![moved("refs/heads/feature", "f2", "f1")],
            vanish: ["f2"].map(str::to_owned).into(),
            ..Outcome::default()
        };
        let view = build(&list, &outcome, Some("refs/heads/feature"));
        assert_eq!(view.commits.len(), list.len());
        assert_eq!(view.added, 0);
        assert_eq!(view.commits[0].parents, ["f1"]);
        assert_eq!(view.rows[0].kind, Kind::Real);
        let discarding = Outcome {
            discards_work: true,
            ..outcome.clone()
        };
        let view_discarding = build(&list, &discarding, Some("refs/heads/feature"));
        assert_eq!(view_discarding.rows[0].kind, Kind::Going);
        let f1 = view.index["f1"];
        assert_eq!(view.commits[f1].refs, ["HEAD -> feature"]);
        assert_eq!(view.rows[view.index["f2"]].kind, Kind::Going);
        assert!(
            view.rows
                .iter()
                .all(|r| r.through.is_empty() && r.incoming.is_empty())
        );
        // The lane from f2 down to f1 goes with f2: faded through m1, into f1's dot.
        let lane = view.graph.rows[view.index["f2"]].col;
        assert!(view.rows[view.index["m1"]].going_through.contains(&lane));
        assert!(view.rows[f1].going_incoming.contains(&lane));
    }

    /// A deleted branch keeps its pill where it is, struck out; a remote one (`origin/x`) moved
    /// by a force push is marked by its own name, not the local branch's.
    #[test]
    fn deleted_and_remote_refs_are_marked() {
        let outcome = Outcome {
            moves: vec![
                RefMove {
                    full: "refs/heads/main".to_owned(),
                    from: Some("m2".to_owned()),
                    to: None,
                },
                moved("refs/remotes/origin/feature", "m1", "f2"),
            ],
            ..Outcome::default()
        };
        let mut list = rows();
        list[2] = c("m1", &["base"], &["origin/feature"]);
        let view = build(&list, &outcome, Some("refs/heads/feature"));
        assert!(view.gone.contains("refs/heads/main"));
        assert_eq!(view.commits[0].refs, ["main"]);
        assert!(view.moved.contains("refs/remotes/origin/feature"));
        assert!(!view.moved.contains("refs/heads/feature"));
        assert_eq!(view.commits[1].refs, ["HEAD -> feature", "origin/feature"]);
        assert!(view.commits[2].refs.is_empty());
    }

    /// A fetched commit the graph has already (through another branch) isn't drawn twice.
    #[test]
    fn a_commit_already_drawn_is_not_added_again() {
        let outcome = Outcome {
            created: vec![(*c("m2", &["m1"], &[])).clone()],
            ..Outcome::default()
        };
        let view = build(&rows(), &outcome, None);
        assert_eq!(view.commits.len(), rows().len());
        assert_eq!(view.rows[0].kind, Kind::New);
        assert_eq!(view.commits.iter().filter(|c| c.hash == "m2").count(), 1);
    }
}
