//! Which commits come before which, from the graph as loaded: worked out once when a drag
//! starts, for the dragged item's commit and HEAD, so that lighting up the targets under the
//! pointer every frame costs a lookup each.

use std::collections::HashMap;
use std::sync::Arc;

use super::offers::Ancestry;
use crate::git::Commit;

/// The ancestors and descendants of a few commits among the loaded rows.
pub(crate) struct Reach {
    /// The rows it was worked out on (shared, not copied): once a reload changes them, a row
    /// may hold another commit than when the drag started, and only rows still holding the
    /// same commit are answered for.
    rows: Vec<Arc<Commit>>,
    sets: Vec<Set>,
    /// First-parent reads can omit an arbitrary side path even with both endpoints loaded.
    complete: Option<Vec<bool>>,
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
        first_parent: bool,
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
        let complete = first_parent.then(|| {
            let mut complete = vec![false; commits.len()];
            for i in (0..commits.len()).rev() {
                complete[i] = commits[i]
                    .parents
                    .iter()
                    .all(|p| index.get(p).is_some_and(|&p| p > i && complete[p]));
            }
            complete
        });
        Self {
            rows: commits.to_vec(),
            sets,
            complete,
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
        let (a, b) = (*self.index.get(ancestor)?, *self.index.get(of)?);
        // Both still in the rows they were in when it was worked out.
        let same = |row: usize| {
            matches!(
                (self.commits.get(row), self.reach.rows.get(row)),
                (Some(now), Some(then)) if Arc::ptr_eq(now, then)
            )
        };
        if !same(a) || !same(b) {
            return None;
        }
        for set in &self.reach.sets {
            if !same(set.row) {
                continue;
            }
            if set.hash == of {
                return self.answer(set.ancestors[a], b);
            }
            if set.hash == ancestor {
                return self.answer(set.descendants[b], b);
            }
        }
        None
    }
}

impl GraphAncestry<'_> {
    fn answer(&self, ancestor: bool, of: usize) -> Option<bool> {
        // A positive path is proof even in partial history. An omitted first-parent side
        // path cannot prove a negative and must not turn an ancestor into a cherry-pick.
        (ancestor || self.reach.complete.as_ref().is_none_or(|c| c[of])).then_some(ancestor)
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
        let reach = Reach::new(&commits, &index, &["e", "c"], false);
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

    /// A reload that keeps the number of rows but puts other commits in them (a stash gone, a
    /// fetched commit come in further down) answers nothing about the rows that changed,
    /// rather than reading another commit's answer.
    #[test]
    fn a_reload_with_as_many_rows_knows_nothing_of_the_rows_that_changed() {
        let (commits, index) = rows(&[
            ("h", &["a"]),
            ("a", &["b"]),
            ("s", &["b"]),
            ("b", &["c"]),
            ("c", &[]),
        ]);
        let reach = Reach::new(&commits, &index, &["h"], false);
        let (mut now, _) = rows(&[("f", &["c"])]);
        let mut reloaded: Vec<Arc<Commit>> = commits.clone();
        reloaded[2] = Arc::clone(&reloaded[3]);
        reloaded[3] = now.remove(0);
        let reloaded_index = reloaded
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let known = reach.ancestry(&reloaded, &reloaded_index);
        assert_eq!(known.is_ancestor("f", "h"), None);
        assert_eq!(known.is_ancestor("a", "h"), Some(true));
    }

    #[test]
    fn rows_that_moved_since_know_nothing() {
        let (commits, index) = rows(&[("b", &["a"]), ("a", &[])]);
        let reach = Reach::new(&commits, &index, &["b"], false);
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

#[cfg(test)]
mod first_parent_tests {
    use super::*;
    #[test]
    fn omitted_side_paths_never_prove_negative_ancestry() {
        let rows: Vec<_> = [
            ("head", vec!["base", "omitted"]),
            ("other", vec!["base"]),
            ("base", vec![]),
        ]
        .into_iter()
        .map(|(hash, parents)| {
            Arc::new(Commit {
                hash: hash.into(),
                parents: parents.into_iter().map(str::to_owned).collect(),
                ..Default::default()
            })
        })
        .collect();
        let index = rows
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let reach = Reach::new(&rows, &index, &["head", "other"], true);
        let ancestry = reach.ancestry(&rows, &index);
        assert_eq!(ancestry.is_ancestor("base", "head"), Some(true));
        assert_eq!(ancestry.is_ancestor("other", "head"), None);
        assert_eq!(ancestry.is_ancestor("head", "other"), Some(false));
    }
}
