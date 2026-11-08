//! The branch conflict radar (#175 item 6): which of the branches worked on now touch the same
//! files, and which would conflict if merged, by pairs. An edge's weight is a count of files
//! Git reported (changed on both sides since the merge-base, or left conflicted by a dry-run
//! merge with `git merge-tree`), never a guessed probability.
//!
//! What a pair says is kept by the two tips it was worked out at: read again, a pair whose
//! tips didn't move is taken as it was, and only the others are merged again.

pub mod read;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use super::Provenance;
use crate::collision::{self, Cell, Step};

/// Most branches compared (besides the default branch): 78 pairs with it.
pub const MAX_BRANCHES: usize = 12;
/// A branch committed to within so many days counts as active.
pub const RECENT_DAYS: i64 = 30;
/// Most files read of what one side changed since the merge-base.
pub const MAX_PATHS: usize = 5_000;

/// A branch that may be compared.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub tip: String,
    /// When its tip was committed (seconds since the epoch).
    pub committed: i64,
    /// It is checked out in a worktree.
    pub worktree: bool,
}

/// The active branches of `all`, the default branch `target` aside: those checked out in
/// worktrees, then those committed to in the last [`RECENT_DAYS`] days, the most recent first,
/// at most [`MAX_BRANCHES`]. Why some were left out, when some were.
pub fn select(
    all: &[Candidate],
    target: Option<&str>,
    now: i64,
) -> (Vec<Candidate>, Option<String>) {
    let recent = now - RECENT_DAYS * 24 * 3600;
    let mut active: Vec<&Candidate> = all
        .iter()
        .filter(|c| Some(c.name.as_str()) != target)
        .filter(|c| c.worktree || c.committed >= recent)
        .collect();
    active.sort_by(|a, b| {
        b.worktree
            .cmp(&a.worktree)
            .then(b.committed.cmp(&a.committed))
            .then(a.name.cmp(&b.name))
    });
    let found = active.len();
    let chosen: Vec<Candidate> = active.into_iter().take(MAX_BRANCHES).cloned().collect();
    let cut = (found > chosen.len()).then(|| {
        format!(
            "{} of {found} active branches compared (those in worktrees first, then the most \
             recently committed)",
            chosen.len()
        )
    });
    (chosen, cut)
}

/// What a pair is kept by: its two tips, the lesser first.
pub type Key = (String, String);

pub fn key(a: &str, b: &str) -> Key {
    if a <= b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

/// What Git said of a pair of tips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairResult {
    /// Their merge-base; `None` when they share no history.
    pub base: Option<String>,
    /// Files changed since the merge-base by each tip, keyed by tip.
    pub changed: HashMap<String, usize>,
    /// Files both changed since their merge-base, sorted.
    pub overlap: Vec<String>,
    /// The dry-run merge: the files it leaves conflicted, or why it couldn't be done.
    pub merge: Cell,
    /// Why it is incomplete (a side changed more files than were read), when it is.
    pub partial: Option<String>,
}

impl PairResult {
    /// It couldn't be worked out (the merge or a read failed: a connection dropped, a lock):
    /// shown as it is, and worked out again on the next read rather than reused.
    pub fn failed(&self) -> bool {
        // No merge-base: a read that failed, or histories with nothing in common (worked out
        // again, cheaply, as the two can't be told apart).
        self.merge.is_err()
            || self.base.is_none()
            || (self.changed.is_empty() && self.partial.is_some())
    }

    /// The files the dry-run merge leaves conflicted (none when it couldn't be done).
    pub fn conflicts(&self) -> &[String] {
        match &self.merge {
            Ok(pair) => &pair.files,
            Err(_) => &[],
        }
    }

    /// Why the dry-run merge couldn't be done, when it couldn't.
    pub fn merge_error(&self) -> Option<&str> {
        self.merge.as_ref().err().map(String::as_str)
    }

    /// The overlapping files that merge cleanly.
    pub fn overlap_only(&self) -> Vec<&String> {
        let conflicts: HashSet<&String> = self.conflicts().iter().collect();
        self.overlap
            .iter()
            .filter(|p| !conflicts.contains(p))
            .collect()
    }
}

/// The files in both `a` and `b`, sorted, each once.
pub fn overlap(a: &[String], b: &[String]) -> Vec<String> {
    let a: HashSet<&String> = a.iter().collect();
    let mut both: Vec<String> = b
        .iter()
        .filter(|p| a.contains(p))
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    both.sort();
    both
}

/// What was worked out of pairs of tips, kept between reads.
#[derive(Clone, Debug, Default)]
pub struct Cache {
    pairs: HashMap<Key, PairResult>,
}

impl Cache {
    pub fn get(&self, a: &str, b: &str) -> Option<&PairResult> {
        self.pairs.get(&key(a, b))
    }

    pub fn insert(&mut self, a: &str, b: &str, result: PairResult) {
        self.pairs.insert(key(a, b), result);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// The pairs (by place in `tips`) not worked out at those tips yet, or that failed.
    pub fn missing(&self, tips: &[String]) -> Vec<(usize, usize)> {
        pairs(tips.len())
            .filter(|&(i, j)| self.get(&tips[i], &tips[j]).is_none_or(PairResult::failed))
            .collect()
    }

    /// Forgets the pairs not among `tips`: a tip moved, a branch went.
    pub fn keep_only(&mut self, tips: &[String]) {
        let live: HashSet<Key> = pairs(tips.len())
            .map(|(i, j)| key(&tips[i], &tips[j]))
            .collect();
        self.pairs.retain(|k, _| live.contains(k));
    }
}

/// Every pair of `n` places, `i < j`.
pub fn pairs(n: usize) -> impl Iterator<Item = (usize, usize)> {
    (0..n).flat_map(move |i| (i + 1..n).map(move |j| (i, j)))
}

/// A pair of the radar: two of its nodes and what Git said of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The two nodes, by place (the default branch, when there is one, is last).
    pub a: usize,
    pub b: usize,
    pub result: PairResult,
    /// Worked out by this read (else taken from the last one: its tips didn't move).
    pub fresh: bool,
}

impl Edge {
    /// The files it is drawn as thick as: conflicted ones, else overlapping ones.
    pub fn weight(&self) -> usize {
        match self.result.conflicts().len() {
            0 => self.result.overlap.len(),
            n => n,
        }
    }

    /// There is something to show: an overlap, a conflict, or a merge that couldn't be done.
    pub fn shown(&self) -> bool {
        !self.result.overlap.is_empty()
            || !self.result.conflicts().is_empty()
            || self.result.merge_error().is_some()
    }
}

/// The radar: the branches compared, each pair, and an order to land them in.
#[derive(Clone, Debug, Default)]
pub struct Radar {
    /// The branches, then the default branch when there is one.
    pub nodes: Vec<Candidate>,
    /// Whether the last node is the default branch.
    pub has_target: bool,
    pub edges: Vec<Edge>,
    /// The order to land the branches on the default branch in with the fewest needing
    /// rework (empty without a default branch), by place among the branches.
    pub order: Vec<Step>,
    pub computed: usize,
    pub reused: usize,
    pub provenance: Provenance,
}

impl Radar {
    /// The branches compared (the default branch aside).
    pub fn branches(&self) -> &[Candidate] {
        let n = self.nodes.len() - usize::from(self.has_target);
        &self.nodes[..n]
    }

    pub fn target(&self) -> Option<&Candidate> {
        self.has_target.then(|| self.nodes.last()).flatten()
    }

    /// The edge of nodes `a` and `b`, if there is one.
    #[cfg(test)]
    pub fn edge(&self, a: usize, b: usize) -> Option<&Edge> {
        self.edges
            .iter()
            .find(|e| (e.a, e.b) == (a, b) || (e.a, e.b) == (b, a))
    }

    /// The places of the nodes whose tip moved: what `current` says each branch is at now
    /// (`None`: gone).
    pub fn moved(&self, current: impl Fn(&str) -> Option<String>) -> HashSet<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| current(&n.name).as_deref() != Some(n.tip.as_str()))
            .map(|(i, _)| i)
            .collect()
    }
}

/// The radar of `nodes` (the default branch last when `has_target`), each pair taken from
/// `cache`, which must hold them all: the edges, and the landing order.
pub fn assemble(
    nodes: Vec<Candidate>,
    has_target: bool,
    cache: &Cache,
    fresh: &HashSet<(usize, usize)>,
) -> (Vec<Edge>, Vec<Step>) {
    let edges: Vec<Edge> = pairs(nodes.len())
        .filter_map(|(a, b)| {
            let result = cache.get(&nodes[a].tip, &nodes[b].tip)?.clone();
            Some(Edge {
                a,
                b,
                result,
                fresh: fresh.contains(&(a, b)),
            })
        })
        .collect();
    let order = if has_target {
        landing_order(&nodes, &edges)
    } else {
        Vec::new()
    };
    (edges, order)
}

/// [`collision::landing_order`] of the branches, onto the last node.
fn landing_order(nodes: &[Candidate], edges: &[Edge]) -> Vec<Step> {
    let n = nodes.len().saturating_sub(1);
    let cell = |a: usize, b: usize| -> Cell {
        edges
            .iter()
            .find(|e| (e.a, e.b) == (a.min(b), a.max(b)))
            .map(|e| e.result.merge.clone())
            .unwrap_or_else(|| Err("not merged".to_owned()))
    };
    let mut cells = vec![vec![None; n]; n];
    for (i, j) in pairs(n) {
        let c = cell(i, j);
        cells[j][i] = Some(c.clone());
        cells[i][j] = Some(c);
    }
    let matrix = collision::Matrix {
        branches: nodes[..n].iter().map(|c| c.name.clone()).collect(),
        target: nodes.last().map(|c| c.name.clone()).unwrap_or_default(),
        cells,
        with_target: (0..n).map(|i| cell(i, n)).collect(),
    };
    collision::landing_order(&matrix)
}

/// A conflicted region of a file as the dry-run merge wrote it: between `<<<<<<<` and
/// `>>>>>>>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hunk {
    /// The line its `<<<<<<<` is on, from 1.
    pub line: usize,
    /// The first side's lines, and the second's.
    pub ours: Vec<String>,
    pub theirs: Vec<String>,
    /// Lines were left out of a side past [`HUNK_LINES`].
    pub cut: bool,
}

/// Most lines kept of each side of a hunk.
pub const HUNK_LINES: usize = 40;
/// Most hunks kept of a file.
pub const FILE_HUNKS: usize = 12;

/// The conflicted regions of `text`, a file with conflict markers; whether more were there
/// than kept.
pub fn parse_hunks(text: &str) -> (Vec<Hunk>, bool) {
    #[derive(PartialEq)]
    enum Side {
        Out,
        Ours,
        Base,
        Theirs,
    }
    let mut hunks = Vec::new();
    let mut more = false;
    let mut side = Side::Out;
    let mut hunk = Hunk::default();
    for (n, line) in text.lines().enumerate() {
        match side {
            Side::Out if line.starts_with("<<<<<<<") => {
                hunk = Hunk {
                    line: n + 1,
                    ..Hunk::default()
                };
                side = Side::Ours;
            }
            Side::Out => {}
            _ if line.starts_with(">>>>>>>") => {
                side = Side::Out;
                if hunks.len() < FILE_HUNKS {
                    hunks.push(std::mem::take(&mut hunk));
                } else {
                    more = true;
                }
            }
            Side::Ours if line.starts_with("|||||||") => side = Side::Base,
            Side::Ours | Side::Base if line.starts_with("=======") => side = Side::Theirs,
            Side::Base => {}
            Side::Ours | Side::Theirs => {
                let lines = if side == Side::Ours {
                    &mut hunk.ours
                } else {
                    &mut hunk.theirs
                };
                if lines.len() < HUNK_LINES {
                    lines.push(line.to_owned());
                } else {
                    hunk.cut = true;
                }
            }
        }
    }
    (hunks, more)
}
