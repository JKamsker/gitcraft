//! Lane layout for the commit graph.
//!
//! Commits arrive newest first. Each lane "waits" for a commit hash; a commit takes the lane that
//! waits for it (or a free one if it's a branch tip), then hands the lane to its first parent and
//! opens or joins lanes for its other parents. Lanes keep their column for their whole life, so
//! lines that pass through a row are always vertical.

use std::borrow::Borrow;
use std::collections::HashMap;

use crate::git::Commit;

#[derive(Clone, Debug, Default)]
pub struct Row {
    /// Column of the commit's dot.
    pub col: usize,
    /// Stable colour identities for the lanes crossing this row.
    pub colours: HashMap<usize, usize>,
    /// Identity of this continuous lane run, distinct when a column is reused.
    pub origin: usize,
    /// Lanes drawn straight through the row, top to bottom.
    pub through: Vec<usize>,
    /// Lanes that come from the top edge into the dot (children of this commit).
    pub incoming: Vec<usize>,
    /// Lanes that leave the dot towards the bottom edge (parents of this commit).
    pub outgoing: Vec<usize>,
}

pub struct Graph {
    pub rows: Vec<Row>,
    /// Widest the graph gets, in lanes.
    pub width: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct LaneHint {
    pub band: usize,
    pub colour: usize,
}

pub fn layout<C: Borrow<Commit>>(commits: &[C]) -> Graph {
    layout_banded(commits, &HashMap::new())
}

/// Keep each band contiguous while preserving every parent edge and continuous lane identity.
pub fn layout_banded<C: Borrow<Commit>>(
    commits: &[C],
    hints: &HashMap<String, LaneHint>,
) -> Graph {
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut bands = Vec::new();
    let mut keys = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    let mut origins = Vec::new();
    let mut next_origin = 0;
    let mut width = 0;

    for (row_index, commit) in commits.iter().enumerate() {
        let commit: &Commit = commit.borrow();
        let hash = commit.hash.as_str();
        let incoming: Vec<usize> = (0..lanes.len())
            .filter(|&i| lanes[i] == Some(hash))
            .collect();
        let hint = hints.get(hash).copied().unwrap_or(LaneHint { band: 0, colour: 0 });
        let col = match incoming.first() {
            Some(&c) => c,
            None => band_lane(&mut lanes, &mut bands, hint.band),
        };
        keys.resize(lanes.len(), 0);
        if incoming.is_empty() { keys[col] = hint.colour; }
        origins.resize(lanes.len(), row_index);
        if incoming.is_empty() {
            origins[col] = next_origin;
            next_origin += 1;
        }
        let origin = origins[col];
        let mut colours: HashMap<usize, usize> = (0..lanes.len()).map(|i| (i, if hints.is_empty() { i } else { keys[i] })).collect();
        let through: Vec<usize> = (0..lanes.len())
            .filter(|&i| lanes[i].is_some() && lanes[i] != Some(hash))
            .collect();
        for &i in &incoming {
            lanes[i] = None;
        }

        let mut outgoing = Vec::new();
        for (n, parent) in commit.parents.iter().enumerate() {
            let parent = parent.as_str();
            if n == 0 {
                // The first parent continues this lane. If another lane already waits for the
                // same parent, both run down and meet at the parent's dot (leftmost lane wins).
                lanes[col] = Some(parent);
                outgoing.push(col);
            } else if let Some(existing) = lanes.iter().position(|l| *l == Some(parent)) {
                outgoing.push(existing);
            } else {
                let parent_hint = hints.get(parent).copied().unwrap_or(hint);
                let lane = band_lane(&mut lanes, &mut bands, parent_hint.band);
                keys.resize(lanes.len(), 0);
                keys[lane] = parent_hint.colour;
                colours.insert(lane, if hints.is_empty() { lane } else { keys[lane] });
                lanes[lane] = Some(parent);
                origins.resize(lanes.len(), row_index);
                origins[lane] = next_origin;
                next_origin += 1;
                outgoing.push(lane);
            }
        }

        while hints.is_empty() && lanes.last() == Some(&None) {
            lanes.pop();
            bands.pop();
        }
        width = width.max(lanes.len()).max(col + 1);
        rows.push(Row {
            col,
            colours,
            origin,
            through,
            incoming,
            outgoing,
        });
    }

    if !hints.is_empty() {
        let mut order: Vec<usize> = (0..bands.len()).collect();
        order.sort_by_key(|&i| (bands[i], i));
        let mut remap = vec![0; bands.len()];
        for (new, old) in order.into_iter().enumerate() { remap[old] = new; }
        for row in &mut rows {
            row.col = remap[row.col];
            for lane in row.through.iter_mut().chain(&mut row.incoming).chain(&mut row.outgoing) { *lane = remap[*lane]; }
            row.colours = std::mem::take(&mut row.colours).into_iter().map(|(col,key)| (remap[col],key)).collect();
        }
        width = bands.len();
    }
    Graph { rows, width }
}

fn band_lane<'a>(lanes: &mut Vec<Option<&'a str>>, bands: &mut Vec<usize>, band: usize) -> usize {
    if let Some(i) = lanes.iter().enumerate().position(|(i,l)| l.is_none() && bands[i] == band) {
        i
    } else {
        lanes.push(None);
        bands.push(band);
        lanes.len() - 1
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
        assert_eq!(g.rows[0].outgoing, vec![0, 1]);
        assert_eq!(g.rows[1].col, 1);
        assert_eq!(g.rows[1].through, vec![0]);
        assert_eq!(g.rows[2].col, 0);
        assert_eq!(g.rows[2].outgoing, vec![0]);
        // Both lanes wait for `a` and meet at its dot, in the leftmost lane.
        assert_eq!(g.rows[3].incoming, vec![0, 1]);
        assert_eq!(g.rows[3].col, 0);
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
}
