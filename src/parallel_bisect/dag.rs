//! The commits still in question, held in memory: read once from git (`log --topo-order`
//! from the bad commit, not from the good ones), then every round is planned without asking
//! git again.

use std::collections::HashMap;

/// The commits of a bisect's range: reachable from the bad commit and from none of the good
/// ones, children before their parents (as `git log --topo-order` lists them).
#[derive(Clone, Debug, Default)]
pub struct Dag {
    hashes: Vec<String>,
    trees: Vec<String>,
    /// Each commit's parents inside the range, by index: always after it.
    parents: Vec<Vec<usize>>,
    index: HashMap<String, usize>,
}

impl Dag {
    /// The range from `(commit, tree, parents)`, children first. Parents outside the range (the
    /// good side) are left out.
    pub fn new(rows: Vec<(String, String, Vec<String>)>) -> Result<Self, String> {
        let index: HashMap<String, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, (hash, _, _))| (hash.clone(), i))
            .collect();
        if index.len() != rows.len() {
            return Err("git listed a commit of the range twice".to_owned());
        }
        let mut dag = Dag {
            index,
            ..Dag::default()
        };
        for (i, (hash, tree, parents)) in rows.into_iter().enumerate() {
            let inside: Vec<usize> = parents
                .iter()
                .filter_map(|p| dag.index.get(p).copied())
                .collect();
            if inside.iter().any(|&p| p <= i) {
                return Err(format!(
                    "git listed a parent of {} before it; the range can't be bisected",
                    super::short(&hash)
                ));
            }
            dag.hashes.push(hash);
            dag.trees.push(tree);
            dag.parents.push(inside);
        }
        Ok(dag)
    }

    /// The range from what `git log --format='%H %T %P'` printed, one commit a line.
    pub fn parse(log: &str) -> Result<Self, String> {
        let mut rows = Vec::new();
        for line in log.lines().filter(|l| !l.trim().is_empty()) {
            let mut words = line.split_whitespace().map(str::to_owned);
            let (Some(hash), Some(tree)) = (words.next(), words.next()) else {
                return Err(format!("git printed a line the bisect can't read: {line}"));
            };
            rows.push((hash, tree, words.collect()));
        }
        Self::new(rows)
    }

    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn index_of(&self, hash: &str) -> Option<usize> {
        self.index.get(hash).copied()
    }

    pub fn hash(&self, i: usize) -> &str {
        &self.hashes[i]
    }

    pub fn tree(&self, i: usize) -> &str {
        &self.trees[i]
    }

    /// The commits whose tree is `tree`.
    pub fn with_tree<'a>(&'a self, tree: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.trees
            .iter()
            .enumerate()
            .filter(move |(_, t)| *t == tree)
            .map(|(i, _)| i)
    }

    /// `i` and its ancestors, among those `within` holds.
    pub fn ancestors(&self, i: usize, within: &[bool]) -> Vec<bool> {
        let mut seen = vec![false; self.len()];
        if !within.get(i).copied().unwrap_or(false) {
            return seen;
        }
        let mut todo = vec![i];
        seen[i] = true;
        while let Some(c) = todo.pop() {
            for &p in &self.parents[c] {
                if within[p] && !seen[p] {
                    seen[p] = true;
                    todo.push(p);
                }
            }
        }
        seen
    }

    /// For each commit `within` holds, how many of them it reaches (itself included): what is
    /// left when it tests bad. `within` has to be a bisect's candidates, where a parent left out
    /// has no ancestor left in: then a commit with one parent in reaches one more than it.
    /// Only merges are walked. 0 for those not within.
    pub fn weights(&self, within: &[bool]) -> Vec<usize> {
        let mut weights = vec![0; self.len()];
        for i in (0..self.len()).rev() {
            if !within[i] {
                continue;
            }
            let inside: Vec<usize> = self.parents[i]
                .iter()
                .copied()
                .filter(|&p| within[p])
                .collect();
            weights[i] = match inside.as_slice() {
                [] => 1,
                [p] => weights[*p] + 1,
                _ => self.ancestors(i, within).iter().filter(|&&a| a).count(),
            };
        }
        weights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A range from `(name, parents)`, children first; every tree its commit's name.
    pub(crate) fn dag(rows: &[(&str, &[&str])]) -> Dag {
        let rows = rows
            .iter()
            .map(|(h, ps)| {
                (
                    (*h).to_owned(),
                    format!("t-{h}"),
                    ps.iter().map(|p| (*p).to_owned()).collect(),
                )
            })
            .collect();
        Dag::new(rows).unwrap_or_default()
    }

    #[test]
    fn reads_what_git_log_prints() -> Result<(), String> {
        let d = Dag::parse("c tc b\nb tb a outside\na ta outside\n\n")?;
        assert_eq!(d.len(), 3);
        assert_eq!(d.index_of("b"), Some(1));
        assert_eq!(d.tree(2), "ta");
        // The parent outside the range is left out.
        assert_eq!(d.parents[1], vec![2]);
        assert!(d.parents[2].is_empty());
        assert!(Dag::parse("lonely\n").is_err());
        Ok(())
    }

    #[test]
    fn refuses_parents_listed_before_their_children() {
        let rows = vec![
            ("a".to_owned(), "t".to_owned(), vec![]),
            ("b".to_owned(), "t".to_owned(), vec!["a".to_owned()]),
        ];
        assert!(Dag::new(rows).is_err());
    }

    /// d is a merge of b and c, both on a.
    #[test]
    fn weights_count_what_each_commit_reaches() {
        let d = dag(&[("d", &["b", "c"]), ("b", &["a"]), ("c", &["a"]), ("a", &[])]);
        let all = vec![true; 4];
        assert_eq!(d.weights(&all), vec![4, 2, 2, 1]);
        // Without a (tested good): b and c reach only themselves.
        let without_a = vec![true, true, true, false];
        assert_eq!(d.weights(&without_a), vec![3, 1, 1, 0]);
        let anc = d.ancestors(1, &all);
        assert_eq!(anc, vec![false, true, false, true]);
    }

    #[test]
    fn a_long_line_is_weighed_without_walking_it() {
        let names: Vec<String> = (0..2000).map(|i| format!("c{i}")).collect();
        let rows = names
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let parents = names.get(i + 1).cloned().into_iter().collect();
                (h.clone(), format!("t{i}"), parents)
            })
            .collect();
        let d = Dag::new(rows).unwrap_or_default();
        let w = d.weights(&vec![true; d.len()]);
        assert_eq!(w[0], 2000);
        assert_eq!(w[1999], 1);
    }
}
