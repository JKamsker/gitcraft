//! The agent fence: one list per repository of the paths agents must stay out of (secrets,
//! CI workflows, migrations, lockfiles…). gitcraft writes it into each agent's own config in
//! the worktrees, and checks every linked worktree's changes, untracked and ignored files
//! against it: the result, not the rule, is what counts.

mod check;
mod configs;
mod pattern;
mod store;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use check::How;
pub use check::{Violation, check};
pub use configs::{AGENTS, CLAUDE, CURSOR, write_all};
pub use pattern::{Pattern, parse_list};
pub use store::{SUGGESTED, read, write};
