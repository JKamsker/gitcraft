//! The collision matrix for branches worked on side by side (by agents, in worktrees): every
//! pair merged in memory with `git merge-tree`, each with the branch they land on, and the
//! order to land them in with the fewest branches needing rework.

mod matrix;
mod merge_tree;
mod order;

#[cfg(test)]
mod tests;

pub use matrix::{Branch, Cell, Matrix, STOPPED, compute, merge, merges};
#[cfg(test)]
pub use merge_tree::{Pair, simulate};
pub use order::{Step, landing_order};
