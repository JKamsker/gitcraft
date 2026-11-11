//! What each commit of a rewritten branch became (issue #175, item 10): the two versions paired
//! by `git range-diff`, each pair told apart by whether its patch is the same (`git patch-id`,
//! observed) or only similar (range-diff's own matching, inferred), and what range-diff leaves
//! unpaired looked at again for commits folded into one (squashed) or split into several, which
//! range-diff never reports and which is only ever inferred here from the changed lines they
//! share.
//!
//! Read-only: `range-diff`, `log -p` and `patch-id` read the commits, nothing is written.

use std::collections::HashMap;
use std::path::Path;

use super::{Evidence, Provenance};
use crate::git::{self, Pair, PairKind, RangeDiff, Rewrite, Series, SeriesCommit};

/// Commits of each version whose patch is read to match them; beyond, pairs keep only what
/// range-diff says of them.
pub const MAX_PATCHES: usize = 400;
/// Changed lines of one commit compared with others; a bigger commit is compared by its first.
pub const MAX_LINES: usize = 5000;
/// The share of an unpaired commit's changed lines found in one commit of the other version
/// for it to count as folded into it (or split off it).
const FOLDED: f32 = 0.6;
/// The share one of several commits must hold for a split or squash across them to count.
const PART: f32 = 0.2;

/// What became of a commit, or of a pair of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// The same patch and message, in the same place.
    Unchanged,
    /// The same patch, in another place.
    Reordered,
    /// The same patch (same patch-id), another message.
    Reworded,
    /// The patch changed: paired by similarity.
    Modified,
    /// Several old commits folded into one new one.
    Squashed,
    /// One old commit split into several new ones.
    Split,
    /// Only in the old version.
    Dropped,
    /// Only in the new version.
    Added,
}

impl Class {
    pub const ALL: [Class; 8] = [
        Class::Unchanged,
        Class::Reordered,
        Class::Reworded,
        Class::Modified,
        Class::Squashed,
        Class::Split,
        Class::Dropped,
        Class::Added,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Class::Unchanged => "unchanged",
            Class::Reordered => "reordered",
            Class::Reworded => "reworded",
            Class::Modified => "modified",
            Class::Squashed => "squashed",
            Class::Split => "split",
            Class::Dropped => "dropped",
            Class::Added => "added",
        }
    }

    /// The letter it is marked with, beside its colour.
    pub fn glyph(self) -> &'static str {
        match self {
            Class::Unchanged => "=",
            Class::Reordered => "R",
            Class::Reworded => "M",
            Class::Modified => "~",
            Class::Squashed => "S",
            Class::Split => "Y",
            Class::Dropped => "-",
            Class::Added => "+",
        }
    }

    /// What it means, for tooltips and the table.
    pub fn meaning(self) -> &'static str {
        match self {
            Class::Unchanged => "the same patch and message, in the same place",
            Class::Reordered => "the same patch and message, in another place",
            Class::Reworded => "the same patch (same patch-id), another message",
            Class::Modified => "the patch changed; range-diff paired them by similarity",
            Class::Squashed => "folded together with other commits into one",
            Class::Split => "split into several commits",
            Class::Dropped => "only in the old version: nothing in the new one matches it",
            Class::Added => "only in the new version: nothing in the old one matches it",
        }
    }

    /// Which wins when a commit has ribbons of several classes.
    fn weight(self) -> u8 {
        match self {
            Class::Unchanged => 0,
            Class::Reordered => 1,
            Class::Reworded => 2,
            Class::Modified => 3,
            Class::Squashed | Class::Split => 4,
            Class::Dropped | Class::Added => 0,
        }
    }
}

/// A commit of one version and what became of it.
#[derive(Clone, Debug, PartialEq)]
pub struct End {
    pub commit: SeriesCommit,
    pub class: Class,
    /// Its place among the commits changed.
    pub moved: bool,
}

/// A ribbon from an old commit to a new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Ribbon {
    /// Places in [`Ribbons::old`] and [`Ribbons::new`].
    pub old: usize,
    pub new: usize,
    pub class: Class,
    /// Observed: the same patch-id (or range-diff's `=`). Inferred: paired by similarity, or by
    /// the changed lines the two share.
    pub evidence: Evidence,
    pub moved: bool,
    /// The range-diff pair it is, when range-diff paired them.
    pub pair: Option<usize>,
    /// Why the two are joined, in a line.
    pub why: String,
}

/// The two versions as ribbons: old commits oldest first, new ones oldest first, what joins
/// them.
#[derive(Clone, Debug, Default)]
pub struct Ribbons {
    pub old: Vec<End>,
    pub new: Vec<End>,
    pub ribbons: Vec<Ribbon>,
    /// What range-diff said, for the patches of its pairs.
    pub diff: RangeDiff,
    /// Both series compared.
    pub series: Option<(Series, Series)>,
    pub provenance: Provenance,
}

impl Ribbons {
    /// How many commits of each class, the old version's unpaired ones as dropped and the new
    /// one's as added.
    pub fn counts(&self) -> Vec<(Class, usize)> {
        Class::ALL
            .iter()
            .map(|&c| {
                let n = match c {
                    Class::Dropped => self.old.iter().filter(|e| e.class == c).count(),
                    Class::Added => self.new.iter().filter(|e| e.class == c).count(),
                    _ => self.ribbons.iter().filter(|r| r.class == c).count(),
                };
                (c, n)
            })
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    /// The ribbons from or to the commit at `at` of a side (`old`: the old version).
    pub fn ribbons_of(&self, old: bool, at: usize) -> impl Iterator<Item = (usize, &Ribbon)> {
        self.ribbons
            .iter()
            .enumerate()
            .filter(move |(_, r)| if old { r.old == at } else { r.new == at })
    }
}

/// A commit's patch as far as matching goes: its patch-id and its changed lines (`+` or `-`,
/// its file, and the line trimmed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    pub id: Option<String>,
    pub lines: Vec<String>,
}

mod classify;
pub use classify::classify;
#[cfg(test)]
use classify::message_changed;

mod read;
#[cfg(test)]
use read::parse_patches;
#[cfg(test)]
pub use read::ribbon_patch;
pub use read::{PrSeen, Source, Tracking, alone_patch, pr_series, read, sources};

#[cfg(test)]
pub(crate) mod tests;
