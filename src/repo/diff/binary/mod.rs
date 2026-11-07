//! Binary files in a diff, instead of "Binary files differ": both versions read (up to a size),
//! what kind of file they are and what can be told about them, the text of documents
//! (Word, Excel, PowerPoint, OpenDocument, PDF) diffed line by line, an archive's members
//! added, removed and changed, and the first bytes of each side in hex.

mod extract;
mod sniff;
mod view;
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
fn read_side(repo: &Path, rev: Option<&str>, path: &str) -> Result<Option<Side>, String> {
    match rev {
        None => match crate::remote::read_within(repo, path, MAX_READ) {
            Some(read) => match read {
                Ok(Some(bytes)) => Ok(Some(side(path, bytes.len() as u64, &bytes))),
                // Refused: too big, or not a file there at all (deleted, a link).
                Ok(None) => match crate::remote::try_is_file(&crate::remote::join(repo, path)) {
                    Ok(true) => Ok(Some(too_big(None))),
                    Ok(false) => Ok(None),
                    Err(e) => Err(format!("{path}: {e}")),
                },
                Err(e) => Err(format!("{path}: {e}")),
            },
            None => match read_local(&repo.join(path), MAX_READ) {
                Ok(Some(bytes)) => Ok(Some(side(path, bytes.len() as u64, &bytes))),
                Ok(None) => Ok(Some(too_big(None))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{path}: {e}")),
            },
        },
        Some(r) => {
            let spec = if r.is_empty() {
                format!(":0:{path}")
            } else {
                format!("{r}:{path}")
            };
            let size: u64 = match git::run(Some(repo), &["cat-file", "-s", &spec]) {
                Ok(n) => n.trim().parse().unwrap_or(0),
                // Not at that revision: the side without the file. There but unreadable (a
                // partial clone without it, a damaged repository): a failure.
                Err(e) => {
                    return match exists_at(repo, r, path) {
                        Ok(false) => Ok(None),
                        Ok(true) => Err(format!("{path}: {e}")),
                        Err(e) => Err(format!("{path}: {e}")),
                    };
                }
            };
            if size > MAX_READ {
                return Ok(Some(too_big(Some(size))));
            }
            let bytes = git::file_at(repo, rev, path)?;
            Ok(Some(side(path, size, &bytes)))
        }
    }
}

/// Whether `path` is in revision `rev` (`""` the index, stage 0); a revision that doesn't
/// exist (the parent of a first commit) has no files.
fn exists_at(repo: &Path, rev: &str, path: &str) -> Result<bool, String> {
    if rev.is_empty() {
        let literal = format!(":(literal){path}");
        let out = git::run(Some(repo), &["ls-files", "-s", "-z", "--", &literal])?;
        return Ok(out.split('\0').any(|e| {
            e.split_once('\t')
                .is_some_and(|(info, p)| p == path && info.ends_with(" 0"))
        }));
    }
    let tree = format!("{rev}^{{tree}}");
    if git::run(Some(repo), &["rev-parse", "--verify", "-q", &tree]).is_err() {
        return Ok(false);
    }
    let out = git::run(
        Some(repo),
        &["ls-tree", "-z", "--full-tree", &tree, "--", path],
    )?;
    Ok(out
        .split('\0')
        .any(|e| e.split_once('\t').is_some_and(|(_, p)| p == path)))
}

/// A file of this machine's working tree as git sees it ([`git::worktree_file`]), read up to
/// `max` bytes and no more: `Ok(None)` when it is larger, though it may have grown since it
/// was listed.
fn read_local(file: &Path, max: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(file)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(file)?
            .into_os_string()
            .into_encoded_bytes();
        return Ok(Some(target));
    }
    if !meta.is_file() {
        return Err(std::io::Error::other("not a file"));
    }
    if meta.len() > max {
        return Ok(None);
    }
    // Checked again on what was opened: a link put there since would be followed by `open`.
    let opened = std::fs::File::open(file)?;
    if !opened.metadata()?.is_file() {
        return Err(std::io::Error::other("not a file"));
    }
    let mut bytes = Vec::new();
    opened.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= max).then_some(bytes))
}

fn too_big(size: Option<u64>) -> Side {
    Side {
        size,
        kind: None,
        facts: Vec::new(),
        head: Vec::new(),
        text: None,
        text_gaps: Vec::new(),
        archive: Archive::No,
        failed: false,
    }
}

/// A side that couldn't be read.
fn failed() -> Side {
    Side {
        failed: true,
        ..too_big(None)
    }
}

fn side(path: &str, size: u64, bytes: &[u8]) -> Side {
    let (kind, facts) = sniff::describe(path, bytes);
    let text = extract::document_text(path, bytes);
    let archive = if !zip::is_zip(bytes) {
        Archive::No
    } else {
        match zip::directory(bytes) {
            Some(d) if !d.members.is_empty() || d.complete => Archive::Listed(d),
            _ => Archive::Unreadable,
        }
    };
    let (text, text_gaps) = match text {
        Some(t) => (Some(t.text), t.gaps),
        None => (None, Vec::new()),
    };
    Side {
        size: Some(size),
        kind: Some(kind),
        facts,
        head: bytes[..bytes.len().min(HEX_BYTES)].to_vec(),
        text,
        text_gaps,
        archive,
        failed: false,
    }
}

/// Both versions of the binary file of `key`, under its old name before a rename.
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
                "the old version"
            } else {
                "the new version"
            };
            text_note = Some(format!(
                "The text of {which} can't be read, so it isn't compared."
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
        for (s, which) in [(&old, "the old version"), (&new, "the new version")] {
            let gaps = s
                .as_ref()
                .map(|s| s.text_gaps.as_slice())
                .unwrap_or_default();
            if !gaps.is_empty() {
                notes.push(format!(
                    "The text of {which} is incomplete ({}), so this comparison may show changes that aren't there.",
                    gaps.join("; ")
                ));
            }
        }
    }
    if let Some(lines) = &text
        && approximate(lines)
    {
        notes.push(
            "The texts are too unlike to match line by line: everything between the first and the last change is shown removed and added."
                .to_owned(),
        );
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
        _ if s.failed => (Listing::Unknown("couldn't be read"), false),
        _ if s.kind.is_none() => (Listing::Unknown("is too big to read"), false),
        Archive::No => (Listing::None, false),
        Archive::Unreadable => (
            Listing::Unknown("has a member list that can't be read"),
            true,
        ),
        Archive::Listed(d) if !d.complete => (Listing::Unknown("has a damaged member list"), true),
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
            let note = format!("The {which} version {why}, so its members aren't compared.");
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
            notes.push(format!(
                "The {which} version has more than one member named {}: the first of each is compared.",
                d.duplicates.join(", ")
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
