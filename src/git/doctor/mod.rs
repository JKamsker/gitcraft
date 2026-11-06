//! What the repository doctor reads and changes: where each setting comes from, and the
//! tracked files git was told not to look at. Everything goes through [`crate::git::run`],
//! so it works the same for a repository here, in WSL or over SSH.

pub mod config;
pub mod flags;

#[cfg(test)]
pub(crate) mod tests;
