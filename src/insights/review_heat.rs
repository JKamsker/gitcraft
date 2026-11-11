//! How a pull request's files and hunks stand for review (issue #175, item 7): which of them
//! the version last seen in gitcraft already showed as they are now, which changed since, and
//! where the open review threads are. Each signal is its own: an unknown one stays unknown
//! (never "failing"), a file not seen here is only that (never "unsafe").
//!
//! "Seen in gitcraft" is the version of the pull request gitcraft remembers being looked at
//! (see [`git::reviewed`]): kept when its page is first opened, its versions compared, or it is
//! marked as seen. gitcraft doesn't know which lines were read, and this view never marks
//! anything as seen. Once that version is no longer part of the head (a force push, a rebase),
//! what was seen can't be matched line by line and is said to be stale.
//!
//! Read-only: `merge-base`, `diff` and `for-each-ref` read the repository; nothing is fetched.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{Evidence, Provenance};
use crate::git;
use crate::pr_threads::Thread;
use crate::pull_request::{DiffSide, PrDetails};

/// Files of the pull request shown; past them it is partial.
pub const MAX_FILES: usize = 300;
/// Hunks of one file shown; past them the file is partial.
pub const MAX_HUNKS: usize = 100;

/// A hunk of a diff, as `@@ -old_start,old_len +new_start,new_len @@` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_len: u32,
    pub new_start: u32,
    pub new_len: u32,
}

impl Hunk {
    /// The lines it covers on the new side; one that only removes covers the line it follows.
    fn new_range(&self) -> (u32, u32) {
        (self.new_start, self.new_start + self.new_len.max(1))
    }

    fn old_range(&self) -> (u32, u32) {
        (self.old_start, self.old_start + self.old_len.max(1))
    }

    /// Lines removed and added.
    pub fn size(&self) -> u32 {
        self.old_len + self.new_len
    }

    pub fn label(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start, self.old_len, self.new_start, self.new_len
        )
    }
}

fn overlaps(a: (u32, u32), b: (u32, u32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// One file of a diff: its name (and the one before a rename), git's status letter, its line
/// counts, and its hunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: char,
    pub added: Option<u32>,
    pub removed: Option<u32>,
    pub hunks: Vec<Hunk>,
}

/// What gitcraft remembers of the version seen, against the head now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    /// Nothing to tell from: no forge selected, or the refs couldn't be read (why).
    Unknown(String),
    /// No version of it was seen in gitcraft.
    Never,
    /// The version seen is part of the head: the hunks changed since, by file (new side).
    Fresh {
        tip: String,
        since: HashMap<String, Vec<Hunk>>,
    },
    /// The version seen is not part of the head (force-pushed, rebased): the files whose
    /// content differs from that version.
    Stale {
        tip: String,
        differing: HashSet<String>,
    },
}

/// What the version seen says of a file or hunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeenMark {
    /// As it was in the version seen: nothing changed since.
    Same,
    /// Changed since the version seen.
    Changed,
    /// No version was seen in gitcraft.
    NotSeen,
    /// The version seen was rewritten, and this differs from it: can't be matched.
    Stale,
    /// The version seen was rewritten, but the file's content is the same as then.
    SameAfterRewrite,
    /// Nothing to tell from.
    Unknown,
}

impl SeenMark {
    pub fn label(self) -> &'static str {
        match self {
            SeenMark::Same => "as seen: unchanged since the version seen in gitcraft",
            SeenMark::Changed => "changed since the version seen in gitcraft",
            SeenMark::NotSeen => "not seen here: no version of it was looked at in gitcraft",
            SeenMark::Stale => {
                "stale: the version seen was rewritten (force push), and this differs from it"
            }
            SeenMark::SameAfterRewrite => {
                "the version seen was rewritten, but this file's content is the same as then"
            }
            SeenMark::Unknown => "unknown: nothing tells what was seen",
        }
    }

    pub fn evidence(self) -> Evidence {
        match self {
            SeenMark::Same | SeenMark::Changed | SeenMark::NotSeen | SeenMark::Stale => {
                Evidence::Observed
            }
            SeenMark::SameAfterRewrite => Evidence::Inferred,
            SeenMark::Unknown => Evidence::Unknown,
        }
    }
}

/// One hunk of the pull request and its signals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkHeat {
    pub hunk: Hunk,
    pub seen: SeenMark,
    /// Lines changed since the version seen, in hunks touching this one (`None`: unknown).
    pub since: Option<u32>,
    /// The threads on its lines (places in [`Heat::threads`]).
    pub threads: Vec<usize>,
}

/// One file of the pull request and its signals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHeat {
    pub file: FileDiff,
    pub seen: SeenMark,
    pub since: Option<u32>,
    /// Every thread on the file, on its hunks or not.
    pub threads: Vec<usize>,
    pub hunks: Vec<HunkHeat>,
    /// It has more hunks than are shown.
    pub more_hunks: bool,
}

/// A review thread, as far as this needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadAt {
    pub path: String,
    pub line: Option<u64>,
    /// On the old side of the diff (a removed line).
    pub left: bool,
    pub resolved: bool,
    pub outdated: bool,
    /// Who started it and the start of what they said.
    pub author: String,
    pub text: String,
    pub url: String,
}

impl ThreadAt {
    pub fn from_thread(thread: &Thread) -> Self {
        let first = thread.comments.first();
        let text: String = first
            .map(|c| {
                c.body
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            path: thread.path.clone(),
            line: thread.line,
            left: thread.side == DiffSide::Left,
            resolved: thread.resolved,
            outdated: thread.outdated,
            author: first.map(|c| c.author.clone()).unwrap_or_default(),
            text,
            url: first.map(|c| c.url.clone()).unwrap_or_default(),
        }
    }

    /// Whether it is on a line of `hunk`.
    fn in_hunk(&self, hunk: &Hunk) -> bool {
        let Some(line) = self.line.filter(|_| !self.outdated) else {
            return false;
        };
        let line = line.min(u32::MAX as u64) as u32;
        let range = if self.left {
            hunk.old_range()
        } else {
            hunk.new_range()
        };
        range.0 <= line && line < range.1
    }
}

/// The pull request's review state by file and hunk.
#[derive(Clone, Debug, Default)]
pub struct Heat {
    pub number: u64,
    /// Where its commits start (the merge base) and its head.
    pub from: String,
    pub head: String,
    pub files: Vec<FileHeat>,
    /// Every thread read; `None` when they couldn't be read.
    pub threads: Option<Vec<ThreadAt>>,
    /// The threads' line numbers belong to another head than the one read: where they are
    /// is a guess.
    pub threads_elsewhere: bool,
    /// The version seen in gitcraft.
    pub seen: Option<Seen>,
    /// Why the version seen is stale, said for the reader.
    pub stale_review: Option<String>,
    pub provenance: Provenance,
}

impl Heat {
    /// The tip of the version seen, if one was.
    #[cfg(test)]
    pub fn seen_tip(&self) -> Option<&str> {
        match &self.seen {
            Some(Seen::Fresh { tip, .. } | Seen::Stale { tip, .. }) => Some(tip),
            _ => None,
        }
    }

    /// How many of `threads` are unresolved.
    pub fn open(&self, threads: &[usize]) -> usize {
        let all = self.threads.as_deref().unwrap_or(&[]);
        threads
            .iter()
            .filter(|&&i| all.get(i).is_some_and(|t| !t.resolved))
            .count()
    }
}

pub use signals::derive;
mod signals;

mod read;
pub use read::{analyze, head_name};
#[cfg(test)]
use read::{parse_hunks, stale_reason};

#[cfg(test)]
pub(crate) mod tests;
