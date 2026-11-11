//! Repeated edits (#175 item 8): the edits of a commit or a range grouped by what they do, so
//! that the same change made in many places shows as one pattern, and an edit that resembles a
//! pattern but was made otherwise shows as its deviation.
//!
//! The matching is on normalized tokens, not on syntax: each edit's removed and added text is
//! tokenized with whitespace dropped, and the names, numbers and strings both sides share are
//! made placeholders. Edits with the same signature form a pattern; placeholders bound to the
//! same text in all of a pattern's edits are shown as that text. No edit is merged into a
//! pattern it doesn't match: a deviation keeps a group of its own, marked as such, and an edit
//! made once is a pattern of one.

pub mod diff;
pub mod pattern;
#[cfg(test)]
mod tests;

use std::path::Path;

use self::diff::Hunk;
use super::Provenance;
use crate::git;

/// How much is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_files: usize,
    /// Files with more lines changed than this aren't read (a generated file, a lock file).
    pub max_file_lines: u32,
    pub max_hunks: usize,
    /// Edits with more tokens on a side than this aren't compared: each is its own group.
    pub max_tokens: usize,
    /// At most so many edits are grouped (a hunk replacing as many lines as it removed is an
    /// edit per line, so hunks alone don't bound them).
    pub max_edits: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 300,
            max_file_lines: 3000,
            max_hunks: 3000,
            max_tokens: 400,
            max_edits: 6000,
        }
    }
}

/// What the edits are read of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A commit against its first parent (a root commit against nothing).
    Commit(String),
    /// HEAD since its merge base with `base`.
    Branch { base: String },
    /// A pull request's head since its merge base with its base: the first of `bases` (its
    /// remote branch, its local one) that is here.
    Pr {
        number: u64,
        head: String,
        bases: Vec<String>,
    },
}

/// What is compared, resolved to commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    pub base: String,
    pub head: String,
    /// The commit, when one commit is compared with its parent: its diffs open as the commit's.
    pub commit: Option<String>,
    /// What it is, in words.
    pub scope: String,
    /// The names it was resolved from, and where they were.
    pub tips: Vec<(String, String)>,
}

/// The name of the tip of a commit's subject: the commit selected, followed as it changes.
pub const SELECTED: &str = "selected commit";

/// Works out what `ask` compares.
pub fn resolve(repo: &Path, ask: &Ask) -> Result<Subject, String> {
    match ask {
        Ask::Commit(rev) => {
            let hash = git::resolve(repo, rev)?;
            let parents = git::run(Some(repo), &["rev-list", "--parents", "-n", "1", &hash])?;
            let (base, scope) = match parents.split_whitespace().nth(1) {
                Some(parent) => (
                    parent.to_owned(),
                    format!("{} against its first parent", short(&hash)),
                ),
                None => (
                    empty_tree(repo)?,
                    format!("{} (a root commit) against nothing", short(&hash)),
                ),
            };
            Ok(Subject {
                base,
                head: hash.clone(),
                commit: Some(hash.clone()),
                scope,
                tips: vec![(SELECTED.to_owned(), hash)],
            })
        }
        Ask::Branch { base } => {
            let head = git::resolve(repo, "HEAD")?;
            let mut candidates = vec![base.clone()];
            if !base.contains('/') {
                candidates.push(format!("origin/{base}"));
            }
            let (base, base_tip, from) = super::fork_point(repo, &head, &candidates)
                .ok_or_else(|| format!("{base} isn't here, or shares no history with HEAD"))?;
            if from == head {
                return Err(format!("HEAD has no commits of its own over {base}"));
            }
            Ok(Subject {
                base: from,
                head: head.clone(),
                commit: None,
                scope: format!("HEAD since its merge base with {base}"),
                tips: vec![("HEAD".to_owned(), head), (base.clone(), base_tip)],
            })
        }
        Ask::Pr {
            number,
            head,
            bases,
        } => {
            let tip = git::resolve(repo, head).map_err(|_| {
                format!(
                    "the head of #{number} ({}) isn't in this repository: fetch it to compare",
                    short(head)
                )
            })?;
            let (base, base_tip, from) = super::fork_point(repo, &tip, bases)
                .ok_or_else(|| format!("the base branch of #{number} isn't here"))?;
            Ok(Subject {
                base: from,
                head: tip.clone(),
                commit: None,
                scope: format!("#{number} since its merge base with {base}"),
                tips: vec![(format!("PR #{number}"), tip), (base, base_tip)],
            })
        }
    }
}

/// The tree with nothing in it, in the repository's hash.
fn empty_tree(repo: &Path) -> Result<String, String> {
    git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")
        .map(|s| s.trim().to_owned())
}

fn short(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// One edit: lines removed and added together (a hunk, or a line of a hunk that replaced as
/// many lines as it removed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// Index into [`Transform::files`].
    pub file: usize,
    /// Index into [`Transform::hunks`].
    pub hunk: usize,
    pub old: Vec<String>,
    pub new: Vec<String>,
    pub old_line: u32,
    pub new_line: u32,
    pub new_len: u32,
}

impl Edit {
    /// Its lines in the new version (where it was, for a removal).
    pub fn lines(&self) -> git::LineRange {
        let start = self.new_line.max(1);
        (start, start + self.new_len.saturating_sub(1))
    }
}

/// What kind of edit a pattern is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Lines replaced by others.
    Rewrite,
    Added,
    Removed,
    /// Only whitespace or line breaks changed.
    Layout,
    /// Too large to compare.
    Large,
}

/// Edits that do the same thing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub shape: Shape,
    /// The removed side and the added side as templates (`foo($a)`, `foo($a, ctx)`).
    pub before: String,
    pub after: String,
    /// Indices into [`Transform::edits`].
    pub edits: Vec<usize>,
    /// The pattern this one resembles but deviates from, an index into [`Transform::patterns`].
    pub deviates_from: Option<usize>,
    /// How many edits deviate from this pattern.
    pub deviating: usize,
    /// Its edits by file: (file, edits), by file.
    pub per_file: Vec<(usize, u32)>,
    /// The edits deviating from it by file.
    pub deviant_files: Vec<(usize, u32)>,
}

impl Pattern {
    /// What it does, in a line.
    pub fn label(&self) -> String {
        match self.shape {
            Shape::Rewrite => format!("{} → {}", self.before, self.after),
            Shape::Added => format!("+ {}", self.after),
            Shape::Removed => format!("− {}", self.before),
            Shape::Layout => "whitespace or line breaks only".to_owned(),
            Shape::Large => format!("large edit: {} → {}", self.before, self.after),
        }
    }

    /// Its edits in `file`.
    pub fn in_file(&self, file: usize) -> u32 {
        count_in(&self.per_file, file)
    }

    /// The edits in `file` that deviate from it.
    pub fn deviants_in(&self, file: usize) -> u32 {
        count_in(&self.deviant_files, file)
    }
}

fn count_in(list: &[(usize, u32)], file: usize) -> u32 {
    list.binary_search_by_key(&file, |(f, _)| *f)
        .map_or(0, |i| list[i].1)
}

/// The edits of a subject, grouped.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub subject: Subject,
    /// The files with edits, by path.
    pub files: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub edits: Vec<Edit>,
    /// Repeated patterns first (the largest first), each followed by those deviating from it;
    /// then what was done once.
    pub patterns: Vec<Pattern>,
    pub provenance: Provenance,
    /// Binary files, which have no lines to compare.
    pub binary: usize,
}

/// The counts the view says in a line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub edits: usize,
    /// Patterns made more than once.
    pub repeated: usize,
    /// Edits of those patterns.
    pub matching: usize,
    /// Edits that deviate from one.
    pub deviating: usize,
    /// Edits like no other.
    pub one_off: usize,
}

impl Transform {
    pub fn summary(&self) -> Summary {
        let mut s = Summary {
            edits: self.edits.len(),
            ..Summary::default()
        };
        for p in &self.patterns {
            if p.deviates_from.is_some() {
                s.deviating += p.edits.len();
            } else if p.edits.len() > 1 {
                s.repeated += 1;
                s.matching += p.edits.len();
            } else {
                s.one_off += p.edits.len();
            }
        }
        s
    }

    /// The edits of pattern `p` in file `f`, and those deviating from it there.
    pub fn cell(&self, p: usize, f: usize) -> (Vec<usize>, Vec<usize>) {
        let Some(pattern) = self.patterns.get(p) else {
            return (Vec::new(), Vec::new());
        };
        let own = pattern
            .edits
            .iter()
            .copied()
            .filter(|&e| self.edits[e].file == f)
            .collect();
        let deviant = self
            .patterns
            .iter()
            .filter(|q| q.deviates_from == Some(p))
            .flat_map(|q| q.edits.iter().copied())
            .filter(|&e| self.edits[e].file == f)
            .collect();
        (own, deviant)
    }
}

/// Reads the edits of `subject` within `limits`, and groups them.
pub fn read(repo: &Path, subject: Subject, limits: &Limits) -> Result<Transform, String> {
    let read = diff::read(repo, &subject.base, &subject.head, limits)?;
    Ok(analyze(subject, read, limits))
}

mod analyze;
pub use analyze::analyze;
