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

/// The signals of each file and hunk of `files`, from what the version seen says (`seen`) and
/// the threads (`None`: not read).
pub fn derive(files: Vec<FileDiff>, seen: &Seen, threads: Option<&[ThreadAt]>) -> Vec<FileHeat> {
    files
        .into_iter()
        .map(|mut file| {
            let more_hunks = file.hunks.len() > MAX_HUNKS;
            file.hunks.truncate(MAX_HUNKS);
            let on_file: Vec<usize> = threads
                .unwrap_or(&[])
                .iter()
                .enumerate()
                .filter(|(_, t)| t.path == file.path)
                .map(|(i, _)| i)
                .collect();
            let (seen_file, since_file) = file_signal(&file, seen);
            let hunks = file
                .hunks
                .iter()
                .map(|h| {
                    let (seen, since) = hunk_signal(&file.path, h, seen, seen_file);
                    let threads = on_file
                        .iter()
                        .copied()
                        .filter(|&i| threads.is_some_and(|all| all[i].in_hunk(h)))
                        .collect();
                    HunkHeat {
                        hunk: *h,
                        seen,
                        since,
                        threads,
                    }
                })
                .collect();
            FileHeat {
                file,
                seen: seen_file,
                since: since_file,
                threads: on_file,
                hunks,
                more_hunks,
            }
        })
        .collect()
}

fn file_signal(file: &FileDiff, seen: &Seen) -> (SeenMark, Option<u32>) {
    match seen {
        Seen::Unknown(_) => (SeenMark::Unknown, None),
        Seen::Never => (SeenMark::NotSeen, None),
        Seen::Fresh { since, .. } => {
            let lines: u32 = since
                .get(&file.path)
                .map_or(0, |hunks| hunks.iter().map(Hunk::size).sum());
            let mark = if since.contains_key(&file.path) {
                SeenMark::Changed
            } else {
                SeenMark::Same
            };
            (mark, Some(lines))
        }
        Seen::Stale { differing, .. } => {
            if differing.contains(&file.path) {
                (SeenMark::Stale, None)
            } else {
                (SeenMark::SameAfterRewrite, None)
            }
        }
    }
}

fn hunk_signal(path: &str, hunk: &Hunk, seen: &Seen, file: SeenMark) -> (SeenMark, Option<u32>) {
    match seen {
        Seen::Fresh { since, .. } => {
            let touching: u32 = since
                .get(path)
                .map(|list| {
                    list.iter()
                        .filter(|s| overlaps(s.new_range(), hunk.new_range()))
                        .map(Hunk::size)
                        .sum()
                })
                .unwrap_or(0);
            let touched = since.get(path).is_some_and(|list| {
                list.iter()
                    .any(|s| overlaps(s.new_range(), hunk.new_range()))
            });
            let mark = if touched {
                SeenMark::Changed
            } else {
                SeenMark::Same
            };
            (mark, Some(touching))
        }
        _ => (file, None),
    }
}

/// `git diff -U0`'s hunks, by file (its new name; its old one when it was deleted).
pub(crate) fn parse_hunks(out: &str) -> HashMap<String, Vec<Hunk>> {
    let mut found: HashMap<String, Vec<Hunk>> = HashMap::new();
    let mut file: Option<String> = None;
    let mut old_name: Option<String> = None;
    let mut in_hunk = false;
    for line in out.lines() {
        if line.starts_with("diff --git ") {
            file = None;
            old_name = None;
            in_hunk = false;
            continue;
        }
        if !in_hunk {
            if let Some(path) = line.strip_prefix("--- a/") {
                old_name = Some(path.trim_end_matches('\t').to_owned());
            } else if let Some(path) = line.strip_prefix("+++ b/") {
                file = Some(path.trim_end_matches('\t').to_owned());
            } else if line == "+++ /dev/null" {
                file = old_name.clone();
            }
        }
        if let Some(rest) = line.strip_prefix("@@ ") {
            in_hunk = true;
            if let (Some(path), Some(hunk)) = (&file, hunk_header(rest)) {
                found.entry(path.clone()).or_default().push(hunk);
            }
        }
    }
    found
}

/// `-12,3 +14,0 @@ fn x()`: the hunk's ranges (a length left out is 1).
fn hunk_header(rest: &str) -> Option<Hunk> {
    let mut parts = rest.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_len) = range(old)?;
    let (new_start, new_len) = range(new)?;
    Some(Hunk {
        old_start,
        old_len,
        new_start,
        new_len,
    })
}

/// The hunks from `from` to `to`, by file.
fn hunks(repo: &Path, from: &str, to: &str) -> Result<HashMap<String, Vec<Hunk>>, String> {
    let out = git::run(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "-U0",
            "-M",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--end-of-options",
            from,
            to,
            "--",
        ],
    )?;
    Ok(parse_hunks(&out))
}

/// Whether `oid` is a whole commit id, as the forges give them.
fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What the version seen of pull request `number` says against `head`. `repo_key`: the forge
/// repository the versions seen are kept for (`None`: no forge selected).
pub fn seen_state(repo: &Path, repo_key: Option<&str>, number: u64, head: &str) -> Seen {
    let Some(key) = repo_key else {
        return Seen::Unknown("no primary forge is selected".to_owned());
    };
    let seen = match git::reviewed(repo, key, number) {
        Ok(Some(seen)) => seen,
        Ok(None) => return Seen::Never,
        Err(e) => return Seen::Unknown(format!("the versions seen couldn't be read: {e}")),
    };
    if seen.tip == head {
        return Seen::Fresh {
            tip: seen.tip,
            since: HashMap::new(),
        };
    }
    if git::is_ancestor(repo, &seen.tip, head) {
        // A file changed without hunks (a binary one, a mode) is changed all the same.
        let found = hunks(repo, &seen.tip, head).and_then(|mut since| {
            for path in changed_paths(repo, &seen.tip, head)? {
                since.entry(path).or_default();
            }
            Ok(since)
        });
        return match found {
            Ok(since) => Seen::Fresh {
                tip: seen.tip,
                since,
            },
            Err(e) => Seen::Unknown(format!("what changed since couldn't be read: {e}")),
        };
    }
    match changed_paths(repo, &seen.tip, head) {
        Ok(differing) => Seen::Stale {
            tip: seen.tip,
            differing,
        },
        Err(e) => Seen::Unknown(format!("the version seen couldn't be compared: {e}")),
    }
}

/// The files whose content differs from `from` to `to`.
fn changed_paths(repo: &Path, from: &str, to: &str) -> Result<HashSet<String>, String> {
    let out = git::run(
        Some(repo),
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--end-of-options",
            from,
            to,
            "--",
        ],
    )?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Why `seen` is stale, for the reader, when it is.
pub fn stale_reason(seen: &Seen, head: &str) -> Option<String> {
    let Seen::Stale { tip, .. } = seen else {
        return None;
    };
    Some(format!(
        "The version last seen in gitcraft ({}) is not part of the head now ({}): the branch \
         was force-pushed or rebased since. What was seen can't be matched hunk by hunk; files \
         whose content is the same as then are marked so (inferred), the rest is stale.",
        crate::repo::short(tip),
        crate::repo::short(head)
    ))
}

/// The pull request `details` (as the forge gave it) by file and hunk, against the version
/// seen. Its commits must be here: nothing is fetched (opening its page fetches them).
pub fn analyze(repo: &Path, details: &PrDetails, repo_key: Option<&str>) -> Result<Heat, String> {
    let (head, base) = (&details.head_oid, &details.base_oid);
    if !is_oid(head) || !is_oid(base) {
        return Err("the forge named commits that aren't commit ids".to_owned());
    }
    let here = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if !here(head) || !here(base) {
        return Err(format!(
            "the commits of #{} aren't in this repository yet: open the pull request's page \
             once, which fetches them",
            details.number
        ));
    }
    let from = git::merge_base(repo, base, head)?;
    let mut changed = git::diff_files(repo, &from, Some(head))?;
    let cut = changed.len() > MAX_FILES;
    changed.truncate(MAX_FILES);
    let mut by_file = hunks(repo, &from, head)?;
    let files: Vec<FileDiff> = changed
        .into_iter()
        .map(|f| FileDiff {
            hunks: by_file.remove(&f.path).unwrap_or_default(),
            path: f.path,
            old_path: f.old_path,
            status: f.status,
            added: f.added,
            removed: f.removed,
        })
        .collect();
    let seen = seen_state(repo, repo_key, details.number, head);
    let threads: Option<Vec<ThreadAt>> = details.inline_error.is_none().then(|| {
        details
            .extras
            .threads
            .iter()
            .map(ThreadAt::from_thread)
            .collect()
    });
    let heat_files = derive(files, &seen, threads.as_deref());
    let threads_elsewhere = threads.is_some()
        && !details.extras.head_oid.is_empty()
        && details.extras.head_oid != *head;
    let mut provenance = Provenance::new(
        "forge (threads) · git diff · versions seen in gitcraft",
        format!(
            "#{}: {} into {}",
            details.number, details.head_branch, details.base_branch
        ),
    )
    .tip(head_name(details.number), head);
    if let Seen::Fresh { tip, .. } | Seen::Stale { tip, .. } = &seen {
        provenance = provenance.tip("seen", tip);
    }
    let mut partial = Vec::new();
    if cut {
        partial.push(format!("only the first {MAX_FILES} files"));
    }
    if heat_files.iter().any(|f| f.more_hunks) {
        partial.push(format!("only the first {MAX_HUNKS} hunks of a file"));
    }
    if let Some(e) = &details.inline_error {
        partial.push(format!("the review threads couldn't be read: {e}"));
    }
    if threads_elsewhere {
        partial.push(
            "the threads' lines were read at another head: where they are is a guess".to_owned(),
        );
    }
    if details.extras.threads.iter().any(|t| t.more_comments) {
        partial.push("some threads have more comments than were read".to_owned());
    }
    if !partial.is_empty() {
        provenance = provenance.partial(partial.join("; "));
    }
    Ok(Heat {
        number: details.number,
        stale_review: stale_reason(&seen, head),
        from,
        head: head.clone(),
        files: heat_files,
        threads,
        threads_elsewhere,
        seen: Some(seen),
        provenance,
    })
}

/// What the provenance calls the head of pull request `number`.
pub fn head_name(number: u64) -> String {
    format!("#{number} head")
}

#[cfg(test)]
pub(crate) mod tests;
