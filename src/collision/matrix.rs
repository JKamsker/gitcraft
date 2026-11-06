//! Every pair of N branches merged in memory, and each with the branch they land on.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::merge_tree::{Pair, simulate};

/// A cell: what the pair conflicts on, or why it couldn't be told.
pub type Cell = Result<Pair, String>;

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
    /// How many files branches `i` and `j` conflict on (0 when clean or unknown).
    pub fn count(&self, i: usize, j: usize) -> usize {
        match self.cells.get(i).and_then(|row| row.get(j)) {
            Some(Some(Ok(pair))) => pair.files.len(),
            _ => 0,
        }
    }

    /// Whether `i` and `j` conflict.
    pub fn conflict(&self, i: usize, j: usize) -> bool {
        self.count(i, j) > 0
    }

    /// How many files branch `i` conflicts on with the target.
    pub fn target_count(&self, i: usize) -> usize {
        match self.with_target.get(i) {
            Some(Ok(pair)) => pair.files.len(),
            _ => 0,
        }
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

/// Merges each pair of `branches`, and each into `target`, counting the merges done in
/// `done`. Stops early with what it has when `cancel` is set to a non-zero value.
pub fn compute(
    repo: &Path,
    branches: &[String],
    target: &str,
    done: &Arc<AtomicUsize>,
    cancel: &Arc<AtomicUsize>,
) -> Result<Matrix, String> {
    let n = branches.len();
    let mut cells = vec![vec![None; n]; n];
    let mut with_target = Vec::with_capacity(n);
    let step = |a: &str, b: &str| {
        if cancel.load(Ordering::Relaxed) != 0 {
            return Err("Stopped".to_owned());
        }
        let cell = simulate(repo, a, b);
        done.fetch_add(1, Ordering::Relaxed);
        Ok(cell)
    };
    for branch in branches {
        with_target.push(step(branch, target)?);
    }
    // A git too old for any of it says so once, rather than in every cell.
    if let Some(Err(e)) = with_target.first()
        && e.contains("needs git")
    {
        return Err(e.clone());
    }
    for i in 0..n {
        for j in i + 1..n {
            let cell = step(&branches[i], &branches[j])?;
            cells[j][i] = Some(cell.clone());
            cells[i][j] = Some(cell);
        }
    }
    Ok(Matrix {
        branches: branches.to_vec(),
        target: target.to_owned(),
        cells,
        with_target,
    })
}
