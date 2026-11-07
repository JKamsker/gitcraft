//! Laying the timeline out as a tree, newest first, the way Vim's undotree does: an entry
//! stays in its parent's lane when it is that parent's first child, and a later child (made
//! after undoing) starts a lane to the right: the first one with nothing in it newer than the
//! parent, so that lanes are used again rather than piling up.

use std::collections::HashMap;

use super::Timeline;

/// One row of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// The entry, `None` for the state before them all (the last row).
    pub(crate) id: Option<u64>,
    pub(crate) lane: usize,
    /// The row of the parent, below this one, and its lane.
    pub(crate) parent: Option<(usize, usize)>,
}

/// The rows, newest first, then the start when there are any; and how many lanes they use.
pub(crate) fn layout(timeline: &Timeline) -> (Vec<Row>, usize) {
    let mut lanes: HashMap<u64, usize> = HashMap::new();
    let mut first_child: HashMap<Option<u64>, u64> = HashMap::new();
    // The newest entry in each lane so far.
    let mut newest: Vec<u64> = Vec::new();
    // Oldest first, so that a parent has its lane before its children.
    for e in &timeline.entries {
        let parent = e.parent.filter(|p| lanes.contains_key(p));
        let lane = match first_child.get(&parent) {
            None => {
                first_child.insert(parent, e.id);
                match parent {
                    Some(p) => lanes.get(&p).copied().unwrap_or(0),
                    None => 0,
                }
            }
            // A free lane: everything in it is older than the parent, so below its row.
            Some(_) => {
                // A first-level one hangs from the bottom row: a lane of its own.
                let after = parent.unwrap_or(0);
                (1..newest.len())
                    .find(|l| newest.get(*l).is_some_and(|n| *n < after))
                    .unwrap_or(newest.len().max(1))
            }
        };
        if newest.len() <= lane {
            newest.resize(lane + 1, 0);
        }
        newest[lane] = e.id;
        lanes.insert(e.id, lane);
    }
    let used = newest.len().saturating_sub(1);
    let count = timeline.entries.len();
    if count == 0 {
        return (Vec::new(), 0);
    }
    // Row of an entry: newest at 0; the start is the last row.
    let row_of: HashMap<u64, usize> = timeline
        .entries
        .iter()
        .rev()
        .enumerate()
        .map(|(row, e)| (e.id, row))
        .collect();
    let start = count;
    let mut rows: Vec<Row> = timeline
        .entries
        .iter()
        .rev()
        .map(|e| {
            let parent = match e.parent.and_then(|p| row_of.get(&p).map(|r| (p, *r))) {
                Some((p, row)) => Some((row, lanes.get(&p).copied().unwrap_or(0))),
                None => Some((start, 0)),
            };
            Row {
                id: Some(e.id),
                lane: lanes.get(&e.id).copied().unwrap_or(0),
                parent,
            }
        })
        .collect();
    rows.push(Row {
        id: None,
        lane: 0,
        parent: None,
    });
    (rows, used + 1)
}
