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

use std::collections::HashMap;
use std::path::Path;

use self::diff::Hunk;
use self::pattern::{Part, Signature, Token};
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
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 300,
            max_file_lines: 3000,
            max_hunks: 3000,
            max_tokens: 400,
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
            let base_tip = git::resolve(repo, base)?;
            let from = git::merge_base(repo, &head, &base_tip)?;
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
            let (base, base_tip) = bases
                .iter()
                .find_map(|b| git::resolve(repo, b).ok().map(|t| (b.clone(), t)))
                .ok_or_else(|| format!("the base branch of #{number} isn't here"))?;
            let from = git::merge_base(repo, &tip, &base_tip)?;
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

/// An edit's tokens and signature, and how it is grouped.
struct Signed {
    shape: Shape,
    old: Vec<Token>,
    new: Vec<Token>,
    sig: Signature,
    key: String,
}

/// A group of edits sharing a key, while they are grouped.
struct Group {
    shape: Shape,
    edits: Vec<usize>,
    before: Vec<Part>,
    after: Vec<Part>,
    /// The first edit's signature.
    sig: Signature,
    /// Its tokens.
    old: Vec<Token>,
    new: Vec<Token>,
    label: String,
}

/// Groups the edits of what was read.
pub fn analyze(subject: Subject, read: diff::Read, limits: &Limits) -> Transform {
    let mut files: Vec<String> = read.hunks.iter().map(|h| h.path.clone()).collect();
    files.sort();
    files.dedup();
    let file_of = |path: &str| {
        files
            .binary_search_by(|f| f.as_str().cmp(path))
            .unwrap_or(0)
    };
    let mut edits = Vec::new();
    for (index, h) in read.hunks.iter().enumerate() {
        let file = file_of(&h.path);
        if h.removed.len() == h.added.len() && h.removed.len() > 1 {
            for (i, (old, new)) in h.removed.iter().zip(&h.added).enumerate() {
                edits.push(Edit {
                    file,
                    hunk: index,
                    old: vec![old.clone()],
                    new: vec![new.clone()],
                    old_line: h.old_start + i as u32,
                    new_line: h.new_start + i as u32,
                    new_len: 1,
                });
            }
        } else {
            edits.push(Edit {
                file,
                hunk: index,
                old: h.removed.clone(),
                new: h.added.clone(),
                old_line: h.old_start,
                new_line: h.new_start,
                new_len: h.new_len,
            });
        }
    }
    let signed: Vec<Signed> = edits
        .iter()
        .enumerate()
        .map(|(i, e)| sign(i, e, limits))
        .collect();
    let groups = group(&edits, &signed);
    let parents = deviations(&groups);
    let patterns = order(&groups, &parents, &edits);
    let mut provenance = Provenance::new("git diff -U0", subject.scope.clone());
    for (name, sha) in &subject.tips {
        provenance = provenance.tip(name.clone(), sha.clone());
    }
    let mut partial = Vec::new();
    if read.beyond > 0 {
        partial.push(format!(
            "only {} of the files changed are read ({} more aren't)",
            limits.max_files, read.beyond
        ));
    }
    if !read.too_large.is_empty() {
        partial.push(format!(
            "{} files with over {} lines changed aren't read",
            read.too_large.len(),
            limits.max_file_lines
        ));
    }
    if read.hunks_cut {
        partial.push(format!(
            "only the first {} hunks are read",
            limits.max_hunks
        ));
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Transform {
        subject,
        files,
        hunks: read.hunks,
        edits,
        patterns,
        provenance,
        binary: read.binary,
    }
}

/// An edit's tokens, shape and key.
fn sign(index: usize, edit: &Edit, limits: &Limits) -> Signed {
    let mut old = Vec::new();
    for line in &edit.old {
        pattern::tokenize(line, &mut old);
    }
    let mut new = Vec::new();
    for line in &edit.new {
        pattern::tokenize(line, &mut new);
    }
    if old.len() > limits.max_tokens || new.len() > limits.max_tokens {
        return Signed {
            shape: Shape::Large,
            old: Vec::new(),
            new: Vec::new(),
            sig: Signature {
                old: Vec::new(),
                new: Vec::new(),
                bindings: Vec::new(),
            },
            key: format!("\u{4}large{index}"),
        };
    }
    let sig = pattern::signature(&old, &new);
    let (shape, key) = if old == new {
        (Shape::Layout, "\u{4}layout".to_owned())
    } else if old.is_empty() {
        (Shape::Added, sig.key())
    } else if new.is_empty() {
        (Shape::Removed, sig.key())
    } else {
        (Shape::Rewrite, sig.key())
    };
    Signed {
        shape,
        old,
        new,
        sig,
        key,
    }
}

/// The edits grouped by key, each group with its template, the largest first.
fn group(edits: &[Edit], signed: &[Signed]) -> Vec<Group> {
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    for (i, s) in signed.iter().enumerate() {
        let g = *index.entry(s.key.as_str()).or_insert_with(|| {
            members.push(Vec::new());
            members.len() - 1
        });
        members[g].push(i);
    }
    let mut groups: Vec<Group> = members
        .into_iter()
        .map(|list| {
            let first = &signed[list[0]];
            let bindings: Vec<&[String]> = list
                .iter()
                .map(|&e| signed[e].sig.bindings.as_slice())
                .collect();
            let (before, after) = pattern::template(&first.sig, &bindings);
            let label = match first.shape {
                Shape::Large | Shape::Layout => {
                    let e = &edits[list[0]];
                    format!("{} → {} lines", e.old.len(), e.new.len())
                }
                _ => format!(
                    "{} → {}",
                    pattern::render_like(&before, &first.old),
                    pattern::render_like(&after, &first.new)
                ),
            };
            Group {
                shape: first.shape,
                edits: list,
                before,
                after,
                sig: first.sig.clone(),
                old: first.old.clone(),
                new: first.new.clone(),
                label,
            }
        })
        .collect();
    groups.sort_by(|a, b| {
        b.edits
            .len()
            .cmp(&a.edits.len())
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.edits[0].cmp(&b.edits[0]))
    });
    groups
}

/// Which group each group deviates from (an index into `groups`), if any: the largest
/// repeated pattern whose removed side it fits while adding something else, or whose
/// substitution it mostly makes.
fn deviations(groups: &[Group]) -> Vec<Option<usize>> {
    let mut parent: Vec<Option<usize>> = vec![None; groups.len()];
    for g in 0..groups.len() {
        let me = &groups[g];
        if matches!(me.shape, Shape::Layout | Shape::Large) {
            continue;
        }
        let (my_old, my_new) = pattern::core(&me.sig);
        let found = (0..g).find(|&p| {
            let other = &groups[p];
            if parent[p].is_some()
                || other.edits.len() < 2
                || other.edits.len() <= me.edits.len()
                || matches!(other.shape, Shape::Layout | Shape::Large)
            {
                return false;
            }
            let fits = me.shape == Shape::Rewrite
                && other.shape == Shape::Rewrite
                && pattern::matches(&other.before, &me.old);
            if fits {
                return true;
            }
            let (old, new) = pattern::core(&other.sig);
            me.shape == other.shape
                && pattern::dice(&me.sig.old, &other.sig.old) >= 0.6
                && pattern::dice(&me.sig.new, &other.sig.new) >= 0.6
                && pattern::dice(
                    &[my_old.clone(), my_new.clone()].concat(),
                    &[old, new].concat(),
                ) >= 0.6
        });
        parent[g] = found;
    }
    parent
}

/// The patterns in the order shown: each repeated pattern followed by its deviations.
fn order(groups: &[Group], parents: &[Option<usize>], edits: &[Edit]) -> Vec<Pattern> {
    let mut sequence = Vec::with_capacity(groups.len());
    for g in 0..groups.len() {
        if parents[g].is_some() {
            continue;
        }
        sequence.push(g);
        sequence.extend((0..groups.len()).filter(|&d| parents[d] == Some(g)));
    }
    let mut at = vec![0; groups.len()];
    for (i, &g) in sequence.iter().enumerate() {
        at[g] = i;
    }
    let by_file = |list: &mut dyn Iterator<Item = usize>| -> Vec<(usize, u32)> {
        let mut counts: Vec<(usize, u32)> = Vec::new();
        let mut files: Vec<usize> = list.map(|e| edits[e].file).collect();
        files.sort_unstable();
        for f in files {
            match counts.last_mut() {
                Some((last, n)) if *last == f => *n += 1,
                _ => counts.push((f, 1)),
            }
        }
        counts
    };
    sequence
        .iter()
        .map(|&g| {
            let group = &groups[g];
            let deviants: Vec<usize> = (0..groups.len())
                .filter(|&d| parents[d] == Some(g))
                .collect();
            let (before, after) = match group.shape {
                Shape::Large | Shape::Layout => {
                    let e = &edits[group.edits[0]];
                    (
                        format!("{} lines", e.old.len()),
                        format!("{} lines", e.new.len()),
                    )
                }
                _ => (
                    pattern::render_like(&group.before, &group.old),
                    pattern::render_like(&group.after, &group.new),
                ),
            };
            Pattern {
                shape: group.shape,
                before,
                after,
                edits: group.edits.clone(),
                deviates_from: parents[g].map(|p| at[p]),
                deviating: deviants.iter().map(|&d| groups[d].edits.len()).sum(),
                per_file: by_file(&mut group.edits.iter().copied()),
                deviant_files: by_file(
                    &mut deviants
                        .iter()
                        .flat_map(|&d| groups[d].edits.iter().copied()),
                ),
            }
        })
        .collect()
}
