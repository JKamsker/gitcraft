//! Which commits come before which, from the graph as loaded: worked out once when a drag
//! starts, for the dragged item's commit and HEAD, so that lighting up the targets under the
//! pointer every frame costs a lookup each.

use std::collections::HashMap;
use std::sync::Arc;

use super::offers::Ancestry;
use crate::git::Commit;

/// The ancestors and descendants of a few commits among the loaded rows.
pub(crate) struct Reach {
    /// How many rows there were: once a reload changes them, the rows found are no longer
    /// where they were, and nothing is known.
    rows: usize,
    sets: Vec<Set>,
}

struct Set {
    hash: String,
    /// Its row.
    row: usize,
    /// Rows it descends from (itself included).
    ancestors: Vec<bool>,
    /// Rows that descend from it (itself included).
    descendants: Vec<bool>,
}

impl Reach {
    /// The reach of each of `hashes` among `commits`, which git lists with no parent before its
    /// children (`--date-order`): every ancestor of a row is further down, and every commit
    /// between a row and one of its ancestors is loaded once that ancestor is.
    pub(crate) fn new(
        commits: &[Arc<Commit>],
        index: &HashMap<String, usize>,
        hashes: &[&str],
    ) -> Self {
        let mut sets: Vec<Set> = Vec::new();
        for hash in hashes {
            let Some(&row) = index.get(*hash) else {
                continue;
            };
            if sets.iter().any(|s| s.row == row) {
                continue;
            }
            sets.push(Set {
                hash: (*hash).to_owned(),
                row,
                ancestors: ancestors(commits, index, row),
                descendants: descendants(commits, index, row),
            });
        }
        Self {
            rows: commits.len(),
            sets,
        }
    }

    /// Answers about `commits` as they are now.
    pub(crate) fn ancestry<'a>(
        &'a self,
        commits: &'a [Arc<Commit>],
        index: &'a HashMap<String, usize>,
    ) -> GraphAncestry<'a> {
        GraphAncestry {
            reach: self,
            commits,
            index,
        }
    }
}

/// Rows `row` descends from: parents are always further down, so one pass down marks them.
fn ancestors(commits: &[Arc<Commit>], index: &HashMap<String, usize>, row: usize) -> Vec<bool> {
    let mut marked = vec![false; commits.len()];
    marked[row] = true;
    for i in row..commits.len() {
        if !marked[i] {
            continue;
        }
        for parent in &commits[i].parents {
            if let Some(&p) = index.get(parent).filter(|&&p| p > i) {
                marked[p] = true;
            }
        }
    }
    marked
}

/// Rows that descend from `row`: those above it with a parent that does (or is it).
fn descendants(commits: &[Arc<Commit>], index: &HashMap<String, usize>, row: usize) -> Vec<bool> {
    let mut marked = vec![false; commits.len()];
    marked[row] = true;
    for i in (0..row).rev() {
        marked[i] = commits[i].parents.iter().any(|parent| {
            index
                .get(parent)
                .is_some_and(|&p| p > i && p < marked.len() && marked[p])
        });
    }
    marked
}

/// [`Reach`] over the rows shown now.
pub(crate) struct GraphAncestry<'a> {
    reach: &'a Reach,
    commits: &'a [Arc<Commit>],
    index: &'a HashMap<String, usize>,
}

impl Ancestry for GraphAncestry<'_> {
    fn is_ancestor(&self, ancestor: &str, of: &str) -> Option<bool> {
        if ancestor == of {
            return Some(true);
        }
        if self.commits.len() != self.reach.rows {
            return None;
        }
        let (a, b) = (*self.index.get(ancestor)?, *self.index.get(of)?);
        for set in &self.reach.sets {
            // Still the row it was: rows shifted by a reload know nothing.
            if self.commits.get(set.row).is_none_or(|c| c.hash != set.hash) {
                continue;
            }
            if set.hash == of {
                return set.ancestors.get(a).copied();
            }
            if set.hash == ancestor {
                return set.descendants.get(b).copied();
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows newest first: `(hash, parents)`.
    fn rows(list: &[(&str, &[&str])]) -> (Vec<Arc<Commit>>, HashMap<String, usize>) {
        let commits: Vec<Arc<Commit>> = list
            .iter()
            .map(|(h, p)| {
                Arc::new(Commit {
                    hash: (*h).to_owned(),
                    parents: p.iter().map(|p| (*p).to_owned()).collect(),
                    ..Commit::default()
                })
            })
            .collect();
        let index = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        (commits, index)
    }

    #[test]
    fn ancestors_and_descendants_of_the_commits_worked_out() {
        // f merges e (on top of c) into d; b is a branch of its own off a; g is not loaded.
        let (commits, index) = rows(&[
            ("f", &["d", "e"]),
            ("e", &["c"]),
            ("d", &["c"]),
            ("b", &["a"]),
            ("c", &["a"]),
            ("a", &["g"]),
        ]);
        let reach = Reach::new(&commits, &index, &["e", "c"]);
        let known = reach.ancestry(&commits, &index);
        assert_eq!(known.is_ancestor("a", "e"), Some(true));
        assert_eq!(known.is_ancestor("d", "e"), Some(false));
        assert_eq!(known.is_ancestor("b", "e"), Some(false));
        assert_eq!(known.is_ancestor("e", "f"), Some(true));
        assert_eq!(known.is_ancestor("e", "d"), Some(false));
        assert_eq!(known.is_ancestor("c", "f"), Some(true));
        assert_eq!(known.is_ancestor("c", "b"), Some(false));
        assert_eq!(known.is_ancestor("e", "e"), Some(true));
        // Neither end worked out, or not loaded.
        assert_eq!(known.is_ancestor("a", "b"), None);
        assert_eq!(known.is_ancestor("g", "e"), None);
    }

    #[test]
    fn rows_that_moved_since_know_nothing() {
        let (commits, index) = rows(&[("b", &["a"]), ("a", &[])]);
        let reach = Reach::new(&commits, &index, &["b"]);
        // A reload put a row on top.
        let (moved, moved_index) = rows(&[("c", &["b"]), ("b", &["a"]), ("a", &[])]);
        assert_eq!(
            reach.ancestry(&moved, &moved_index).is_ancestor("a", "b"),
            None
        );
        assert_eq!(
            reach.ancestry(&commits, &index).is_ancestor("a", "b"),
            Some(true)
        );
    }
}
