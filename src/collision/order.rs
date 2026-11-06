//! The order to land branches in with the fewest of them needing rework.
//!
//! A branch needs rework when it conflicts with one landed before it (or with the target).
//! Branches that conflict with none of each other can all land first, cleanly: the most of
//! them that can (a largest set with no conflict between any two, among those clean with the
//! target) go first, and each of the others conflicts with one of them. The rest follow, the
//! fewest files to resolve (with what landed before and with the target) first.

use super::matrix::Matrix;

/// Most branches whose every subset is tried for the clean set; past it, picked one by one.
const EXACT_UP_TO: usize = 16;

/// A step of the landing order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The branch, by its place in the matrix.
    pub branch: usize,
    /// Conflicted files with the target.
    pub with_target: usize,
    /// The branches landed before this one that it conflicts with, and on how many files.
    pub with_earlier: Vec<(usize, usize)>,
}

impl Step {
    pub fn clean(&self) -> bool {
        self.with_target == 0 && self.with_earlier.is_empty()
    }
}

/// The suggested landing order of the matrix's branches.
pub fn landing_order(m: &Matrix) -> Vec<Step> {
    let n = m.branches.len();
    let candidates: Vec<usize> = (0..n).filter(|&i| m.target_count(i) == 0).collect();
    let mut first = clean_set(m, &candidates);
    // Fewest conflicts with the others first: what they would hit comes later and smaller.
    first.sort_by_key(|&i| (total(m, i), i));
    let mut order: Vec<usize> = first;
    let mut rest: Vec<usize> = (0..n).filter(|i| !order.contains(i)).collect();
    while !rest.is_empty() {
        let pick = rest.iter().copied().min_by_key(|&i| {
            (
                with_landed(m, i, &order) + m.target_count(i),
                total(m, i),
                i,
            )
        });
        let Some(pick) = pick else {
            break;
        };
        rest.retain(|&i| i != pick);
        order.push(pick);
    }
    order
        .iter()
        .enumerate()
        .map(|(at, &branch)| Step {
            branch,
            with_target: m.target_count(branch),
            with_earlier: order[..at]
                .iter()
                .filter(|&&e| m.conflict(branch, e))
                .map(|&e| (e, m.count(branch, e)))
                .collect(),
        })
        .collect()
}

/// Conflicted files of `i` with every other branch.
fn total(m: &Matrix, i: usize) -> usize {
    (0..m.branches.len()).map(|j| m.count(i, j)).sum()
}

/// Conflicted files of `i` with the branches in `landed`.
fn with_landed(m: &Matrix, i: usize, landed: &[usize]) -> usize {
    landed.iter().map(|&j| m.count(i, j)).sum()
}

/// A largest set of `candidates` with no two conflicting: exactly for a few, greedily (fewest
/// conflicts first) for more.
fn clean_set(m: &Matrix, candidates: &[usize]) -> Vec<usize> {
    if candidates.len() <= EXACT_UP_TO {
        exact_clean_set(m, candidates)
    } else {
        greedy_clean_set(m, candidates)
    }
}

fn exact_clean_set(m: &Matrix, candidates: &[usize]) -> Vec<usize> {
    let k = candidates.len();
    // Which candidates each one conflicts with, as bits.
    let conflicts: Vec<u32> = candidates
        .iter()
        .map(|&i| {
            candidates
                .iter()
                .enumerate()
                .filter(|&(_, &j)| m.conflict(i, j))
                .fold(0u32, |bits, (b, _)| bits | (1 << b))
        })
        .collect();
    let mut best = 0u32;
    // Ties go to the set with fewer conflicted files with the rest, then the earlier branches.
    let mut best_key = (0usize, usize::MAX);
    for set in 0u32..(1u32 << k) {
        let independent = (0..k).all(|b| set & (1 << b) == 0 || conflicts[b] & set == 0);
        if !independent {
            continue;
        }
        let size = set.count_ones() as usize;
        let cost: usize = (0..k)
            .filter(|b| set & (1 << b) != 0)
            .map(|b| total(m, candidates[b]))
            .sum();
        if size > best_key.0 || (size == best_key.0 && cost < best_key.1) {
            best = set;
            best_key = (size, cost);
        }
    }
    (0..k)
        .filter(|b| best & (1 << b) != 0)
        .map(|b| candidates[b])
        .collect()
}

fn greedy_clean_set(m: &Matrix, candidates: &[usize]) -> Vec<usize> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by_key(|&i| {
        (
            (0..m.branches.len()).filter(|&j| m.conflict(i, j)).count(),
            i,
        )
    });
    let mut set: Vec<usize> = Vec::new();
    for i in sorted {
        if set.iter().all(|&j| !m.conflict(i, j)) {
            set.push(i);
        }
    }
    set
}
