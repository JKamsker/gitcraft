//! Read-only resource observations. These types confer no deletion authority.
mod classify;
mod preview;
mod read;
#[cfg(test)]
mod tests;
#[cfg(target_os = "linux")]
mod walk;

pub(crate) use preview::{Operation, previews};
pub(crate) use read::collect;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Completeness {
    Complete,
    Partial(String),
    Unavailable(String),
    #[cfg(not(target_os = "linux"))]
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Measurement {
    pub bytes: u64,
    pub files: u64,
    pub status: Completeness,
}
impl Default for Measurement {
    fn default() -> Self {
        Self {
            bytes: 0,
            files: 0,
            status: Completeness::Complete,
        }
    }
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    Tracked,
    Untracked,
    Build,
    Ignored,
    Unclassified,
}
#[cfg(any(target_os = "linux", test))]
impl Category {
    fn index(self) -> usize {
        self as usize
    }
}

/// Disjoint logical sizes; no subtraction of incomplete totals.
#[derive(Clone, Debug)]
pub(crate) struct Inventory {
    pub root: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub collected: i64,
    pub generation: u64,
    pub staged: usize,
    pub unstaged: usize,
    pub conflicted: usize,
    pub untracked: usize,
    pub measurements: [Measurement; 5],
    pub candidates: Vec<PathBuf>,
    pub blockers: Vec<String>,
    pub presence: Option<String>,
    pub fingerprint: String,
    pub publication: String,
}
impl Inventory {
    pub fn complete(&self) -> bool {
        self.measurements
            .iter()
            .all(|m| m.status == Completeness::Complete)
    }
    pub fn logical_bytes(&self) -> u64 {
        self.measurements.iter().map(|m| m.bytes).sum()
    }
}
