//! The search: which commits may still be the first bad one, which to test next (k at once),
//! and when it is over. Pure: the session feeds it verdicts as they come.
//!
//! The candidates are the commits reachable from every commit found bad and from none found
//! good (`git bisect`'s own rule). A bad commit narrows them to its ancestors, a good one takes
//! its ancestors away, a skipped one stays. Exactly one bad commit is always among them: the
//! one found bad last. When no untested one is left, that one is the first bad commit, or (with
//! skipped ones left) one of them is.

use super::dag::Dag;
use super::verdict::Verdict;

/// How a search ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conclusion {
    /// The first bad commit, by index.
    FirstBad(usize),
    /// Skipped commits are left: the first bad commit is one of these (the bad one first).
    OneOf(Vec<usize>),
}

#[derive(Clone, Debug)]
pub struct Search {
    dag: Dag,
    verdicts: Vec<Option<Verdict>>,
    candidate: Vec<bool>,
    /// The bad commit among the candidates.
    tip: usize,
}

impl Search {
    /// A search of `dag` (from `bad`, not from the good commits), `bad` taken as bad.
    pub fn new(dag: Dag, bad: &str) -> Result<Self, String> {
        let Some(tip) = dag.index_of(bad) else {
            return Err(format!(
                "{} is reachable from a good commit: nothing to bisect",
                super::short(bad)
            ));
        };
        let mut verdicts = vec![None; dag.len()];
        verdicts[tip] = Some(Verdict::Bad);
        let candidate = dag.ancestors(tip, &vec![true; dag.len()]);
        Ok(Search {
            dag,
            verdicts,
            candidate,
            tip,
        })
    }

    pub fn dag(&self) -> &Dag {
        &self.dag
    }

    #[cfg(test)]
    pub fn verdict(&self, i: usize) -> Option<Verdict> {
        self.verdicts.get(i).copied().flatten()
    }

    pub fn is_candidate(&self, i: usize) -> bool {
        self.candidate.get(i).copied().unwrap_or(false)
    }

    /// How many commits may still be the first bad one.
    pub fn remaining(&self) -> usize {
        self.candidate.iter().filter(|&&c| c).count()
    }

    /// The candidates, by index.
    pub fn candidates(&self) -> Vec<usize> {
        (0..self.dag.len()).filter(|&i| self.candidate[i]).collect()
    }

    /// The candidates not tested yet.
    fn untested(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.dag.len()).filter(|&i| self.candidate[i] && self.verdicts[i].is_none())
    }

    /// Takes in `verdict` for every commit with `tree` (a commit with the same tree tests the
    /// same); the commits it was taken for. Only candidates change the search: a commit left
    /// out already would never have been tested.
    pub fn mark_tree(&mut self, tree: &str, verdict: Verdict) -> Vec<usize> {
        let same: Vec<usize> = self.dag.with_tree(tree).collect();
        for &i in &same {
            self.mark(i, verdict);
        }
        same
    }

    /// Takes in `verdict` for commit `i`.
    pub fn mark(&mut self, i: usize, verdict: Verdict) {
        if i >= self.dag.len() {
            return;
        }
        // A definite answer stays (the bad end's mark too): a later one for the same commit
        // can only come from a test that answers differently each time.
        if self.verdicts[i].is_some_and(|v| !v.is_skip()) {
            return;
        }
        self.verdicts[i] = Some(verdict);
        if !self.candidate[i] {
            return;
        }
        match verdict {
            Verdict::Good => {
                let gone = self.dag.ancestors(i, &self.candidate);
                for (c, g) in self.candidate.iter_mut().zip(gone) {
                    *c &= !g;
                }
            }
            Verdict::Bad => {
                self.candidate = self.dag.ancestors(i, &self.candidate);
                self.tip = i;
            }
            Verdict::Skip | Verdict::TimedOut => {}
        }
    }

    /// Up to `k` commits to test next, none with a tree of `busy` (being tested) nor two with
    /// the same tree: those that split the candidates most evenly into k + 1 parts. Testing a
    /// commit that reaches w of the n candidates leaves w (bad) or n − w (good), so the j-th
    /// pick is the untested commit whose weight is nearest j·n/(k+1); on a line of commits
    /// that is an even split.
    pub fn picks(&self, k: usize, busy: &[&str]) -> Vec<usize> {
        let mut free: Vec<usize> = self
            .untested()
            .filter(|&i| !busy.contains(&self.dag.tree(i)))
            .collect();
        if k == 0 || free.is_empty() {
            return Vec::new();
        }
        let weights = self.dag.weights(&self.candidate);
        let n = self.remaining() as f64;
        let mut picks = Vec::with_capacity(k);
        for j in 1..=k {
            let target = j as f64 * n / (k + 1) as f64;
            let Some(&pick) = free.iter().min_by(|&&a, &&b| {
                let da = (weights[a] as f64 - target).abs();
                let db = (weights[b] as f64 - target).abs();
                da.total_cmp(&db).then(a.cmp(&b))
            }) else {
                break;
            };
            let tree = self.dag.tree(pick);
            free.retain(|&i| self.dag.tree(i) != tree);
            picks.push(pick);
        }
        picks
    }

    /// About how many more rounds of `k` tests it takes: log base k + 1 of what is untested.
    pub fn rounds_left(&self, k: usize) -> usize {
        let untested = self.untested().count();
        if untested == 0 {
            return 0;
        }
        // The least r with (k + 1)^r ≥ untested + 1, counted in integers (a float log is off
        // at exact powers).
        let parts = k.max(1) + 1;
        let (mut reach, mut rounds) = (1usize, 0);
        while reach < untested + 1 {
            reach = reach.saturating_mul(parts);
            rounds += 1;
        }
        rounds
    }

    /// The end, once no candidate is left untested.
    pub fn conclusion(&self) -> Option<Conclusion> {
        if self.untested().next().is_some() {
            return None;
        }
        let rest: Vec<usize> = self.candidates();
        if rest.len() == 1 {
            return Some(Conclusion::FirstBad(self.tip));
        }
        let mut one_of = vec![self.tip];
        one_of.extend(rest.into_iter().filter(|&i| i != self.tip));
        Some(Conclusion::OneOf(one_of))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parallel_bisect::dag::Dag;

    /// A line of `n` commits, `c{n-1}` the newest (listed first), the parent of `c0` outside.
    fn line(n: usize) -> Dag {
        let rows = (0..n)
            .rev()
            .map(|i| {
                let parents = if i == 0 {
                    vec![]
                } else {
                    vec![format!("c{}", i - 1)]
                };
                (format!("c{i}"), format!("t{i}"), parents)
            })
            .collect();
        Dag::new(rows).unwrap_or_default()
    }

    fn index(s: &Search, name: &str) -> usize {
        s.dag().index_of(name).unwrap_or(usize::MAX)
    }

    /// Runs a search to its end with a test that fails from `culprit` on (by the commits'
    /// numbers); how many rounds it took.
    fn solve(n: usize, k: usize, culprit: usize, skipped: &[usize]) -> (Option<Conclusion>, usize) {
        let Ok(mut s) = Search::new(line(n), &format!("c{}", n - 1)) else {
            return (None, 0);
        };
        let mut rounds = 0;
        loop {
            if let Some(end) = s.conclusion() {
                return (Some(end), rounds);
            }
            let picks = s.picks(k, &[]);
            assert!(!picks.is_empty() && picks.len() <= k, "{picks:?}");
            rounds += 1;
            for i in picks {
                let number: usize = s.dag().hash(i)[1..].parse().unwrap_or(0);
                let v = if skipped.contains(&number) {
                    Verdict::Skip
                } else if number >= culprit {
                    Verdict::Bad
                } else {
                    Verdict::Good
                };
                s.mark(i, v);
            }
            assert!(rounds < 200, "no end");
        }
    }

    #[test]
    fn finds_the_culprit_with_any_parallelism() {
        for k in [1, 2, 3, 4, 8, 16] {
            for culprit in [0, 1, 37, 98, 99] {
                let (end, _) = solve(100, k, culprit, &[]);
                let s = Search::new(line(100), "c99").ok();
                let want = s.map(|s| index(&s, &format!("c{culprit}")));
                assert_eq!(
                    end,
                    want.map(Conclusion::FirstBad),
                    "k {k} culprit {culprit}"
                );
            }
        }
    }

    /// About log base k + 1 of n rounds, not log base 2.
    #[test]
    fn more_at_once_takes_fewer_rounds() {
        let rounds = |k| solve(1000, k, 613, &[]).1;
        assert!(rounds(1) <= 10, "{}", rounds(1));
        assert!(rounds(3) <= 5, "{}", rounds(3));
        assert!(rounds(7) <= 4, "{}", rounds(7));
        assert!(rounds(31) <= 2, "{}", rounds(31));
        assert!(rounds(31) < rounds(7) && rounds(7) < rounds(1));
    }

    #[test]
    fn the_picks_split_a_line_evenly() -> Result<(), String> {
        let s = Search::new(line(100), "c99")?;
        let mut picked: Vec<String> = s
            .picks(3, &[])
            .into_iter()
            .map(|i| s.dag().hash(i).to_owned())
            .collect();
        picked.sort();
        // Weights 25, 50, 75: c24, c49, c74.
        assert_eq!(picked, ["c24", "c49", "c74"]);
        // Those busy are left to their tests.
        let busy = s.picks(3, &[]);
        let trees: Vec<&str> = busy.iter().map(|&i| s.dag().tree(i)).collect();
        let more = s.picks(3, &trees);
        assert!(more.iter().all(|i| !busy.contains(i)), "{more:?}");
        Ok(())
    }

    #[test]
    fn a_skipped_culprit_leaves_a_choice() {
        // c40 can't be tested: the culprit is c40 or c41, as git bisect would say.
        let (end, _) = solve(100, 4, 40, &[40]);
        let s = Search::new(line(100), "c99").ok();
        let want = s.map(|s| vec![index(&s, "c41"), index(&s, "c40")]);
        assert_eq!(end, want.map(Conclusion::OneOf));
        // A skip elsewhere doesn't change the answer.
        let (end, _) = solve(100, 4, 40, &[10, 70, 71]);
        let s = Search::new(line(100), "c99").ok();
        assert_eq!(end, s.map(|s| Conclusion::FirstBad(index(&s, "c40"))));
    }

    #[test]
    fn the_bad_end_alone_is_its_own_culprit() -> Result<(), String> {
        let s = Search::new(line(1), "c0")?;
        assert_eq!(s.conclusion(), Some(Conclusion::FirstBad(0)));
        assert_eq!(s.rounds_left(4), 0);
        assert!(Search::new(line(3), "nope").is_err());
        Ok(())
    }

    /// A commit with the same tree as one tested has its answer too.
    #[test]
    fn a_tree_is_tested_once() -> Result<(), String> {
        let rows = vec![
            ("d".to_owned(), "x".to_owned(), vec!["c".to_owned()]),
            ("c".to_owned(), "y".to_owned(), vec!["b".to_owned()]),
            ("b".to_owned(), "x".to_owned(), vec!["a".to_owned()]),
            ("a".to_owned(), "z".to_owned(), vec![]),
        ];
        let mut s = Search::new(Dag::new(rows)?, "d")?;
        let marked = s.mark_tree("x", Verdict::Good);
        assert_eq!(marked, vec![0, 2]);
        // d stays the bad one (it was marked so); b and a are gone.
        assert_eq!(s.verdict(0), Some(Verdict::Bad));
        assert_eq!(s.candidates(), vec![0, 1]);
        assert_eq!(s.picks(4, &[]), vec![1]);
        Ok(())
    }

    /// A merge: the first bad commit on one side of it is found, the other side cleared.
    #[test]
    fn bisects_across_a_merge() -> Result<(), String> {
        //   m (bad) = merge of x2 and y2;  x1 ← x2,  y1 ← y2, both on base (outside).
        let rows = [
            ("m", vec!["x2", "y2"]),
            ("x2", vec!["x1"]),
            ("y2", vec!["y1"]),
            ("x1", vec!["base"]),
            ("y1", vec!["base"]),
        ];
        let rows = rows
            .into_iter()
            .map(|(h, ps)| {
                (
                    h.to_owned(),
                    format!("t-{h}"),
                    ps.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect();
        let mut s = Search::new(Dag::new(rows)?, "m")?;
        let bad = |h: &str| matches!(h, "m" | "y2");
        for k in [1, 2, 4] {
            let mut s2 = s.clone();
            while s2.conclusion().is_none() {
                for i in s2.picks(k, &[]) {
                    let v = if bad(s2.dag().hash(i)) {
                        Verdict::Bad
                    } else {
                        Verdict::Good
                    };
                    s2.mark(i, v);
                }
            }
            let y2 = index(&s2, "y2");
            assert_eq!(s2.conclusion(), Some(Conclusion::FirstBad(y2)), "k {k}");
        }
        // Neither a skip nor another answer undoes an answer.
        s.mark(1, Verdict::Good);
        s.mark(1, Verdict::Skip);
        s.mark(1, Verdict::Bad);
        assert_eq!(s.verdict(1), Some(Verdict::Good));
        Ok(())
    }

    #[test]
    fn rounds_left_is_log_base_k_plus_one() -> Result<(), String> {
        let s = Search::new(line(1024), "c1023")?;
        assert_eq!(s.rounds_left(1), 10);
        assert_eq!(s.rounds_left(3), 5);
        assert_eq!(s.rounds_left(1023), 1);
        // 124 untested, k = 4: 5^3 = 125, three rounds (a float log says 3.0000000000000004).
        let s = Search::new(line(125), "c124")?;
        assert_eq!(s.rounds_left(4), 3);
        Ok(())
    }
}
