//! Every pair of N branches merged in memory, and each with the branch they land on.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::merge_tree::{Pair, simulate};

/// A cell: what the pair conflicts on, or why it couldn't be told.
pub type Cell = Result<Pair, String>;

/// What a run that was stopped answers.
pub const STOPPED: &str = "Stopped";

/// A branch of the matrix: its name, shown, and the ref git is given (`refs/heads/x`, so a
/// tag of the same name isn't what is merged).
#[derive(Clone, Debug)]
pub struct Branch {
    pub name: String,
    pub git: String,
}

/// The N×N matrix (the diagonal is left `None`) and each branch against the target.
#[derive(Clone, Debug, Default)]
pub struct Matrix {
    pub branches: Vec<String>,
    pub target: String,
    /// `cells[i][j]`, the same as `cells[j][i]`.
    pub cells: Vec<Vec<Option<Cell>>>,
    /// Each branch merged into the target.
    pub with_target: Vec<Cell>,
}

impl Matrix {
    fn cell(&self, i: usize, j: usize) -> Option<&Cell> {
        self.cells.get(i)?.get(j)?.as_ref()
    }

    /// How many files branches `i` and `j` conflict on (0 when clean or unknown).
    pub fn count(&self, i: usize, j: usize) -> usize {
        match self.cell(i, j) {
            Some(Ok(pair)) => pair.files.len(),
            _ => 0,
        }
    }

    /// Whether `i` and `j` conflict.
    pub fn conflict(&self, i: usize, j: usize) -> bool {
        self.count(i, j) > 0
    }

    /// Whether what merging `i` and `j` does couldn't be told.
    pub fn unknown(&self, i: usize, j: usize) -> bool {
        i != j && !matches!(self.cell(i, j), Some(Ok(_)))
    }

    /// How many files branch `i` conflicts on with the target.
    pub fn target_count(&self, i: usize) -> usize {
        match self.with_target.get(i) {
            Some(Ok(pair)) => pair.files.len(),
            _ => 0,
        }
    }

    /// Whether what merging `i` into the target does couldn't be told.
    pub fn target_unknown(&self, i: usize) -> bool {
        !matches!(self.with_target.get(i), Some(Ok(_)))
    }

    /// The first error any cell had, to show above the matrix.
    pub fn first_error(&self) -> Option<&str> {
        self.with_target
            .iter()
            .chain(self.cells.iter().flatten().flatten())
            .find_map(|c| c.as_ref().err().map(String::as_str))
    }
}

/// How many merges a matrix of `n` branches takes: each pair, and each with the target.
pub fn merges(n: usize) -> usize {
    n * n.saturating_sub(1) / 2 + n
}

/// Most merges run at once (each its own git, or its own shell on another machine).
const AT_ONCE: usize = 4;

/// Merges each pair of `branches`, and each into `target`, counting the merges done in
/// `done`. When `cancel` is set it stops after the merges under way, answering [`STOPPED`].
pub fn compute(
    repo: &Path,
    branches: &[Branch],
    target: &Branch,
    done: &Arc<AtomicUsize>,
    cancel: &Arc<AtomicBool>,
) -> Result<Matrix, String> {
    let n = branches.len();
    // Each branch with the target first (`j == n`), then every pair.
    let mut jobs: Vec<(usize, usize)> = (0..n).map(|i| (i, n)).collect();
    jobs.extend((0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))));
    let of = |j: usize| if j == n { target } else { &branches[j] };
    // A git too old for any of it says so once, rather than in every cell.
    if let Some(first) = branches.first() {
        let cell = merge(repo, first, target);
        if let Err(e) = &cell
            && e.contains("needs git")
        {
            return Err(e.clone());
        }
    }
    let next = AtomicUsize::new(0);
    let results: Vec<Vec<((usize, usize), Cell)>> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..AT_ONCE.min(jobs.len()))
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    while !cancel.load(Ordering::Relaxed) {
                        let Some(&(i, j)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            break;
                        };
                        mine.push(((i, j), merge(repo, &branches[i], of(j))));
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                    mine
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().unwrap_or_default())
            .collect()
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(STOPPED.to_owned());
    }
    let mut cells = vec![vec![None; n]; n];
    let mut with_target: Vec<Cell> = vec![Err("Not merged".to_owned()); n];
    for ((i, j), cell) in results.into_iter().flatten() {
        if j == n {
            with_target[i] = cell;
        } else {
            cells[j][i] = Some(cell.clone());
            cells[i][j] = Some(cell);
        }
    }
    Ok(Matrix {
        branches: branches.iter().map(|b| b.name.clone()).collect(),
        target: target.name.clone(),
        cells,
        with_target,
    })
}

/// [`simulate`], its error naming the branches by their names.
fn merge(repo: &Path, a: &Branch, b: &Branch) -> Cell {
    simulate(repo, &a.git, &b.git).map_err(|e| e.replace(&a.git, &a.name).replace(&b.git, &b.name))
}
