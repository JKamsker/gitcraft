//! Tests run over history: results stored by tree in git notes as git-test stores them, the
//! scratch worktrees they run in, and a runner that tests many commits at once.

#[cfg(test)]
pub(crate) mod fixture;
mod notes;
mod runner;
mod scratch;
#[cfg(test)]
mod tests;

pub use notes::*;
pub use runner::*;
pub use scratch::*;
