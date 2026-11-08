//! Parallel bisect: the first bad commit between a good and a bad one, found by testing k
//! commits at once, each in its own worktree on the machine chosen (this one, a WSL
//! distribution, an SSH host), one shell each. That takes about log base k + 1 of n rounds
//! instead of log base 2 of n. Its parts:
//! - [`search`]: which commits may still be the culprit, and which k to test next (pure);
//! - [`session`]: the rounds, on a thread of their own, telling the tab what happens;
//! - [`workspace`] and [`runner`]: the worktrees on that machine, and the tests run there;
//! - [`cache`]: answers by tree, so that no tree is tested twice.

pub(crate) mod bundle;
mod cache;
mod dag;
pub(crate) mod display;
mod machine;
pub(crate) mod observe;
mod runner;
mod search;
mod session;
mod verdict;
mod workspace;

#[cfg(test)]
mod tests;

pub use cache::TreeCache;
pub use machine::Machine;
pub(crate) use session::shutdown;
pub use session::{Event, Handle, Outcome, Setup, start};
pub use verdict::Verdict;

/// The most commits tested at once.
pub const MAX_PARALLEL: usize = 32;

/// How many commits to test at once on a machine with `cores` cores, unless told otherwise:
/// one per core, but no more than 16 (each test has its own worktree, and may use several
/// cores itself).
pub fn default_parallel(cores: usize) -> usize {
    cores.clamp(1, 16)
}

/// A hash as messages show it.
pub(crate) fn short(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// Removes, in the background, what a parallel bisect that crashed left in `repo` (its
/// worktrees, its folder): when a tab opens on it. A bisect running there (in this window or
/// another) is left alone.
pub fn clear_leftovers(repo: std::path::PathBuf) {
    let Ok(pool) = crate::tasks::pool() else {
        return;
    };
    // Shared workers propagate cancellation to every nested process operation and are
    // joined before process/remote services shut down. A full queue preserves leftovers.
    let _ = pool.submit(
        crate::tasks::cancellation().child(),
        crate::tasks::Priority::Background,
        Some(repo.clone()),
        Box::new(move || workspace::clear_leftovers(&repo)),
    );
}
