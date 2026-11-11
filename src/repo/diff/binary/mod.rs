//! Binary files in a diff, instead of "Binary files differ": both versions read (up to a size),
//! what kind of file they are and what can be told about them, the text of documents
//! (Word, Excel, PowerPoint, OpenDocument, PDF) diffed line by line, an archive's members
//! added, removed and changed, and the first bytes of each side in hex.

mod extract;
mod search;
mod sniff;
mod view;
pub(super) use search::document as search_document;
mod zip;

#[cfg(test)]
pub(in crate::repo) use extract::tests::{make_docx, make_pdf};
#[cfg(test)]
pub(in crate::repo) use zip::tests::make_zip;

use std::path::Path;

pub(super) use view::{binary_view, textconv_note};

use super::DiffKey;
use super::line_diff::{Edit, diff_lines};
use crate::git::{self, Area};

/// Past this size a side isn't read: only its size is told.
pub(super) const MAX_READ: u64 = 64 * 1024 * 1024;
/// How many bytes of each side the hex view shows.
pub(super) const HEX_BYTES: usize = 64 * 1024;

/// One version of a binary file.
pub struct Side {
    /// Its size, when known (one too big to read elsewhere may not say).
    pub size: Option<u64>,
    /// "PDF document", "ZIP archive"…; `None` when it was too big to read.
    pub kind: Option<String>,
    pub facts: Vec<sniff::Fact>,
    /// Its first [`HEX_BYTES`] bytes.
    pub head: Vec<u8>,
    pub text: Option<String>,
    /// What of its text couldn't be read (a slide too big…): its text is then incomplete.
    pub text_gaps: Vec<String>,
    pub archive: Archive,
    /// Whether reading it failed: then nothing is known of it, not even whether it is there.
    pub failed: bool,
}

/// What a side is as an archive.
#[derive(Debug)]
pub enum Archive {
    /// Not one (or not read).
    No,
    /// One whose member list can't be read.
    Unreadable,
    Listed(zip::Directory),
}

/// A member of an archive, and how it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberChange {
    pub name: String,
    /// `None` when it isn't on that side.
    pub old: Option<u64>,
    pub new: Option<u64>,
}

/// Both sides of a binary file's diff (`None` for a side without the file), and what changed.
pub struct BinaryPair {
    pub old: Option<Side>,
    pub new: Option<Side>,
    /// The text of both sides diffed, when every side there has text (lines cut to a length
    /// that shows), and how many lines changed.
    pub text: Option<Vec<(Edit, String)>>,
    pub text_changes: usize,
    /// Why the text isn't diffed although a side has some, or why its diff isn't the whole
    /// story (a side's text incomplete, the texts too unlike to match line by line).
    pub text_note: Option<String>,
    /// The members of both archives that differ.
    pub members: Option<Vec<MemberChange>>,
    /// Why the members aren't compared although a side is an archive, or what to know of the
    /// comparison (a name given to two members).
    pub members_note: Option<String>,
    /// Why a side couldn't be read.
    pub error: Option<String>,
}

/// Whether git's patch says the file is binary.
pub(super) fn is_binary_patch(text: &str) -> bool {
    text.lines()
        .take(12)
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch")
}

/// One version of `path`: at `rev` (`Some("")` the index, `None` the working tree). `Ok(None)`
/// when the file isn't there; an error when it couldn't be read.
pub(super) fn load_pair(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    renamed_from: Option<&str>,
) -> BinaryPair {
    let old_path = match key {
        DiffKey::Work(file) => file.old_path.as_deref(),
        _ => None,
    }
    .or(renamed_from)
    .unwrap_or(path);
    let (old, new) = match key {
        DiffKey::Remerge { hash, .. } => (Ok(None), read_side(repo, Some(hash), path)),
        DiffKey::Commit { hash, .. } => (
            read_side(repo, Some(&format!("{hash}^")), old_path),
            read_side(repo, Some(hash), path),
        ),
        DiffKey::Work(file) => match file.area {
            Area::Staged => (
                read_side(repo, Some("HEAD"), old_path),
                read_side(repo, Some(""), path),
            ),
            Area::Unstaged => (read_side(repo, Some(""), path), read_side(repo, None, path)),
            Area::Untracked | Area::Conflicted => (Ok(None), read_side(repo, None, path)),
        },
        DiffKey::Range { from, to, .. } => (
            read_side(repo, Some(from), old_path),
            read_side(repo, to.as_deref(), path),
        ),
    };
    let error = old.as_ref().err().or(new.as_ref().err()).cloned();
    // A side that couldn't be read stays a side: not taken for one without the file.
    let known = |s: Result<Option<Side>, String>| s.unwrap_or_else(|_| Some(failed()));
    pair(known(old), known(new), error)
}

/// The pair, with what changed between its sides worked out.
pub(super) fn pair(old: Option<Side>, new: Option<Side>, error: Option<String>) -> BinaryPair {
    fn text_of(s: &Option<Side>) -> Option<&str> {
        s.as_ref().and_then(|s| s.text.as_deref())
    }
    let unread = |s: &Option<Side>| s.is_some() && text_of(s).is_none();
    let (mut text, mut text_changes, mut text_note) = (None, 0, None);
    if text_of(&old).is_some() || text_of(&new).is_some() {
        if unread(&old) || unread(&new) {
            let which = if unread(&old) {
                crate::i18n::text("repo-core-3900")
            } else {
                crate::i18n::text("repo-core-3901")
            };
            text_note = Some(crate::i18n::message(
                "repo-core-3902",
                &[("which", which.to_string().into())],
            ));
        } else {
            let a: Vec<&str> = text_of(&old)
                .map(|t| t.lines().collect())
                .unwrap_or_default();
            let b: Vec<&str> = text_of(&new)
                .map(|t| t.lines().collect())
                .unwrap_or_default();
            let diffed: Vec<(Edit, String)> = diff_lines(&a, &b)
                .into_iter()
                .map(|(e, l)| (e, cut(l)))
                .collect();
            text_changes = diffed.iter().filter(|(e, _)| *e != Edit::Same).count();
            text = Some(diffed);
        }
    }
    let mut notes: Vec<String> = text_note.into_iter().collect();
    if text.is_some() {
        for (s, which) in [
            (&old, crate::i18n::text("repo-core-3900")),
            (&new, crate::i18n::text("repo-core-3901")),
        ] {
            let gaps = s
                .as_ref()
                .map(|s| s.text_gaps.as_slice())
                .unwrap_or_default();
            if !gaps.is_empty() {
                notes.push(crate::i18n::message(
                    "repo-core-3903",
                    &[
                        ("which", which.to_string().into()),
                        ("gaps", gaps.join("; ").to_string().into()),
                    ],
                ));
            }
        }
    }
    if let Some(lines) = &text
        && approximate(lines)
    {
        notes.push(crate::i18n::text("repo-core-3904").to_owned());
    }
    let text_note = (!notes.is_empty()).then(|| notes.join(" "));
    let (members, members_note) = compare_members(&old, &new);
    BinaryPair {
        old,
        new,
        text,
        text_changes,
        text_note,
        members,
        members_note,
        error,
    }
}

/// Whether the diff is the fallback [`diff_lines`] makes for texts too unlike to search: past
/// the lines both start and end with, all removed then all added, though a line is on both
/// sides. A real shortest diff of such lines would have kept one.
fn approximate(lines: &[(Edit, String)]) -> bool {
    let head = lines.iter().take_while(|(e, _)| *e == Edit::Same).count();
    let tail = lines[head..]
        .iter()
        .rev()
        .take_while(|(e, _)| *e == Edit::Same)
        .count();
    let middle = &lines[head..lines.len() - tail];
    if middle.iter().any(|(e, _)| *e == Edit::Same) {
        return false;
    }
    let removed: std::collections::HashSet<&str> = middle
        .iter()
        .filter(|(e, _)| *e == Edit::Del)
        .map(|(_, l)| l.as_str())
        .collect();
    middle
        .iter()
        .any(|(e, l)| *e == Edit::Add && removed.contains(l.as_str()))
}

/// What a side is for comparing members.
enum Listing<'a> {
    /// Without the file, or not an archive: no members.
    None,
    Listed(&'a zip::Directory),
    /// Its members aren't known, and why.
    Unknown(&'static str),
}

fn listing(s: &Option<Side>) -> (Listing<'_>, bool) {
    let Some(s) = s else {
        return (Listing::None, false);
    };
    match &s.archive {
        _ if s.failed => (Listing::Unknown(crate::i18n::text("repo-core-1688")), false),
        _ if s.kind.is_none() => (Listing::Unknown(crate::i18n::text("repo-core-3905")), false),
        Archive::No => (Listing::None, false),
        Archive::Unreadable => (Listing::Unknown(crate::i18n::text("repo-core-3906")), true),
        Archive::Listed(d) if !d.complete => {
            (Listing::Unknown(crate::i18n::text("repo-core-3907")), true)
        }
        Archive::Listed(d) => (Listing::Listed(d), true),
    }
}

/// The members changed, when both sides' are known; else why they aren't compared.
fn compare_members(
    old: &Option<Side>,
    new: &Option<Side>,
) -> (Option<Vec<MemberChange>>, Option<String>) {
    let ((a, a_zip), (b, b_zip)) = (listing(old), listing(new));
    if !a_zip && !b_zip {
        return (None, None);
    }
    for (l, which) in [(&a, "old"), (&b, "new")] {
        if let Listing::Unknown(why) = l {
            let note = crate::i18n::message(
                "repo-core-3908",
                &[
                    ("which", which.to_string().into()),
                    ("why", why.to_string().into()),
                ],
            );
            return (None, Some(note));
        }
    }
    fn members<'a>(l: &Listing<'a>) -> &'a [zip::Member] {
        match l {
            Listing::Listed(d) => d.members.as_slice(),
            _ => &[],
        }
    }
    let mut notes = Vec::new();
    for (l, which) in [(&a, "old"), (&b, "new")] {
        if let Listing::Listed(d) = l
            && !d.duplicates.is_empty()
        {
            notes.push(crate::i18n::message(
                "repo-core-3909",
                &[
                    ("which", which.to_string().into()),
                    ("d", d.duplicates.join(", ").to_string().into()),
                ],
            ));
        }
    }
    let note = (!notes.is_empty()).then(|| notes.join(" "));
    (Some(member_changes(members(&a), members(&b))), note)
}

/// A line cut to what a row shows (the whole is in the document).
fn cut(line: &str) -> String {
    const MAX: usize = crate::repo::highlight::MAX_LINE_CHARS;
    match line.char_indices().nth(MAX) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// The members added, removed or changed (by size or checksum), by name; of members with the
/// same name, the first (the one whose text is read).
pub(super) fn member_changes(old: &[zip::Member], new: &[zip::Member]) -> Vec<MemberChange> {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<&str, (Option<&zip::Member>, Option<&zip::Member>)> = BTreeMap::new();
    for m in old {
        by_name.entry(&m.name).or_default().0.get_or_insert(m);
    }
    for m in new {
        by_name.entry(&m.name).or_default().1.get_or_insert(m);
    }
    by_name
        .into_iter()
        .filter(|(_, (a, b))| match (a, b) {
            (Some(a), Some(b)) => a.crc != b.crc || a.size != b.size,
            _ => true,
        })
        .map(|(name, (a, b))| MemberChange {
            name: name.to_owned(),
            old: a.map(|m| m.size),
            new: b.map(|m| m.size),
        })
        .collect()
}

/// Whether the repository's config has any `diff.<driver>.textconv`, asked at most every
/// half minute per repository: when it has none (most), no file's diff goes through one.
fn has_textconv(repo: &Path) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static ASKED: Mutex<Option<HashMap<std::path::PathBuf, (Instant, bool)>>> = Mutex::new(None);
    let fresh = |at: &Instant| at.elapsed() < Duration::from_secs(30);
    let known = ASKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|m| m.get(repo).copied())
        .filter(|(at, _)| fresh(at));
    if let Some((_, any)) = known {
        return any;
    }
    // Exit 1: none set.
    let any = git::run_ok_codes(
        Some(repo),
        &["config", "--get-regexp", r"^diff\..*\.textconv$"],
        &[0, 1],
    )
    .is_ok_and(|out| !out.trim().is_empty());
    ASKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(repo.to_path_buf(), (Instant::now(), any));
    any
}

/// The `textconv` program git turns `path` into text with for diffs, if its `diff` attribute
/// names a driver that has one: the diff shown is then that text, not the file.
pub fn textconv_for(repo: &Path, path: &str) -> Option<String> {
    if !has_textconv(repo) {
        return None;
    }
    if let Ok(textconv) = git::textconv_of(repo, path) {
        return textconv;
    }
    let out = git::run(Some(repo), &["check-attr", "-z", "diff", "--", path]).ok()?;
    let driver = out.split('\0').nth(2)?;
    if matches!(driver, "" | "set" | "unset" | "unspecified") {
        return None;
    }
    let key = format!("diff.{driver}.textconv");
    git::run(Some(repo), &["config", "--get", &key])
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests;

mod read;
#[cfg(test)]
use read::read_local;
use read::{failed, read_side};
