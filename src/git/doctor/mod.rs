//! What the repository doctor reads and changes: where each setting comes from, the
//! tracked files git was told not to look at, ignore rules and attributes. Everything goes through [`crate::git::run`],
//! so it works the same for a repository here, in WSL or over SSH.

pub mod config;
pub mod flags;
pub mod ignore;
pub mod rules;

#[cfg(test)]
mod ignore_tests;
#[cfg(test)]
pub(crate) mod tests;
