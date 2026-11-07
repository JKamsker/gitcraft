//! Lane layout for the commit graph.
//!
//! Commits arrive newest first. Each lane "waits" for a commit hash; a commit takes the lane that
//! waits for it (or a free one if it's a branch tip), then hands the lane to its first parent and
//! opens or joins lanes for its other parents. Lanes keep their column for their whole life, so
//! lines that pass through a row are always vertical.

use std::borrow::Borrow;
use std::sync::Arc;

use crate::git::Commit;

#[derive(Clone, Debug, Default)]
pub struct Row {
    /// Column of the commit's dot.
    pub col: usize,
    /// Identity of this continuous lane run, distinct when a column is reused.
    pub origin: usize,
    /// Lanes drawn straight through the row, top to bottom.
    pub through: Arc<[usize]>,
    /// Lanes that come from the top edge into the dot (children of this commit).
    pub incoming: Box<[usize]>,
    /// Lanes that leave the dot towards the bottom edge (parents of this commit).
    pub outgoing: Box<[usize]>,
}

#[derive(Default)]
pub struct Graph {
    pub rows: Vec<Row>,
    /// Widest the graph gets, in lanes.
    pub width: usize,
}

/// [`layout_with`] of a history read with every parent.
pub fn layout<C: Borrow<Commit>>(commits: &[C]) -> Graph {
    layout_with(commits, false)
}

/// The lanes of `commits`, newest first; read with `--first-parent` when `first_parent`: a merge's other
/// parents aren't listed, so they get no lane (one would run to the bottom of the graph,
/// waiting for a commit that never comes).
pub fn layout_with<C: Borrow<Commit>>(commits: &[C], first_parent: bool) -> Graph {
    let followed = if first_parent { 1 } else { usize::MAX };
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows: Vec<Row> = Vec::with_capacity(commits.len());
    let mut origins = Vec::new();
    let mut next_origin = 0;
    let mut width = 0;

    for (row_index, commit) in commits.iter().enumerate() {
        let commit: &Commit = commit.borrow();
        let hash = commit.hash.as_str();
        let incoming: Vec<usize> = (0..lanes.len())
            .filter(|&i| lanes[i] == Some(hash))
            .collect();
        let col = match incoming.first() {
            Some(&c) => c,
            None => free_lane(&mut lanes),
        };
        origins.resize(lanes.len(), row_index);
        if incoming.is_empty() {
            origins[col] = next_origin;
            next_origin += 1;
        }
        let origin = origins[col];
        let through = (0..lanes.len()).filter(|&i| lanes[i].is_some() && lanes[i] != Some(hash));
        // Along a branch, the other lanes usually pass through many consecutive rows
        // unchanged. Share that immutable list instead of allocating a copy for each row.
        let through = match rows.last() {
            Some(previous) if previous.through.iter().copied().eq(through.clone()) => {
                Arc::clone(&previous.through)
            }
            _ => through.collect(),
        };
        for &i in &incoming {
            lanes[i] = None;
        }

        let mut outgoing = Vec::new();
        for (n, parent) in commit.parents.iter().enumerate().take(followed) {
            let parent = parent.as_str();
            if n == 0 {
                // The first parent continues this lane. If another lane already waits for the
                // same parent, both run down and meet at the parent's dot (leftmost lane wins).
                lanes[col] = Some(parent);
                outgoing.push(col);
            } else if let Some(existing) = lanes.iter().position(|l| *l == Some(parent)) {
                outgoing.push(existing);
            } else {
                let lane = free_lane(&mut lanes);
                lanes[lane] = Some(parent);
                origins.resize(lanes.len(), row_index);
                origins[lane] = next_origin;
                next_origin += 1;
                outgoing.push(lane);
            }
        }

        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        width = width.max(lanes.len()).max(col + 1);
        rows.push(Row {
            col,
            origin,
            through,
            incoming: incoming.into_boxed_slice(),
            outgoing: outgoing.into_boxed_slice(),
        });
    }

    Graph { rows, width }
}

fn free_lane(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.into(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            author: String::new(),
            date: String::new(),
            summary: String::new(),
            refs: Vec::new(),
            ..Commit::default()
        }
    }

    #[test]
    fn linear_history_uses_one_lane() {
        let g = layout(&[c("c", &["b"]), c("b", &["a"]), c("a", &[])]);
        assert_eq!(g.width, 1);
        assert!(g.rows.iter().all(|r| r.col == 0));
    }

    #[test]
    fn merge_and_branch() {
        // m merges f into b; f and b both come from a.
        let g = layout(&[
            c("m", &["b", "f"]),
            c("f", &["a"]),
            c("b", &["a"]),
            c("a", &[]),
        ]);
        assert_eq!(g.width, 2);
        assert_eq!(&*g.rows[0].outgoing, &[0, 1]);
        assert_eq!(g.rows[1].col, 1);
        assert_eq!(&*g.rows[1].through, &[0]);
        assert_eq!(g.rows[2].col, 0);
        assert_eq!(&*g.rows[2].outgoing, &[0]);
        // Both lanes wait for `a` and meet at its dot, in the leftmost lane.
        assert_eq!(&*g.rows[3].incoming, &[0, 1]);
        assert_eq!(g.rows[3].col, 0);
    }

    /// Read with `--first-parent`, a merge's second parent is never listed: no lane waits for
    /// it, so the graph stays one lane wide and nothing runs past the last commit.
    #[test]
    fn first_parent_history_opens_no_lane_for_merged_parents() {
        let commits = [c("m2", &["m1", "f2"]), c("m1", &["a", "f1"]), c("a", &[])];
        let g = layout_with(&commits, true);
        assert_eq!(g.width, 1);
        assert!(g.rows.iter().all(|r| r.col == 0 && r.through.is_empty()));
        assert_eq!(&*g.rows[0].outgoing, &[0]);
        assert!(g.rows[2].outgoing.is_empty());
        // Laid out with every parent, the same rows would leave lanes waiting for good.
        assert!(layout(&commits).width > 1);
    }

    #[test]
    fn lane_names_follow_first_parent_runs_without_leaking_into_reused_columns() {
        let g = layout(&[
            c("merge", &["main", "feature"]),
            c("feature", &["base"]),
            c("main", &["base"]),
            c("base", &[]),
            c("unrelated", &[]),
        ]);
        assert_eq!(g.rows[0].origin, g.rows[2].origin);
        assert_eq!(g.rows[2].origin, g.rows[3].origin);
        assert_ne!(g.rows[1].origin, g.rows[0].origin);
        assert_eq!(g.rows[4].col, g.rows[0].col);
        assert_ne!(g.rows[4].origin, g.rows[0].origin);
    }

    #[test]
    fn a_branch_shares_unchanged_bypass_lanes() {
        let g = layout(&[
            c("merge", &["c", "side"]),
            c("c", &["b"]),
            c("b", &["a"]),
            c("a", &["root"]),
            c("side", &["root"]),
            c("root", &[]),
        ]);
        for i in 1..=3 {
            assert_eq!(g.rows[i].col, 0);
            assert_eq!(&*g.rows[i].through, &[1]);
            assert!(Arc::ptr_eq(&g.rows[1].through, &g.rows[i].through));
        }
        assert_eq!(&*g.rows[4].through, &[0]);
        assert!(!Arc::ptr_eq(&g.rows[3].through, &g.rows[4].through));
        assert!(g.rows[5].through.is_empty());
        assert_eq!(&*g.rows[5].incoming, &[0, 1]);
    }
}
