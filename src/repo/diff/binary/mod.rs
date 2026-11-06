//! Binary files in a diff, instead of "Binary files differ": both versions read (up to a size),
//! what kind of file they are and what can be told about them, the text of documents
//! (Word, Excel, PowerPoint, OpenDocument, PDF) diffed line by line, an archive's members
//! added, removed and changed, and the first bytes of each side in hex.

mod extract;
mod sniff;
mod view;
mod zip;

use std::path::Path;

pub(super) use view::{binary_view, textconv_note};

use super::DiffKey;
use super::line_diff::{Edit, diff_lines};
use crate::git::{self, Area};

/// Past this size a side isn't read: only its size is told.
const MAX_READ: u64 = 64 * 1024 * 1024;
/// How many bytes of each side the hex view shows.
pub(super) const HEX_BYTES: usize = 64 * 1024;

/// One version of a binary file.
pub struct Side {
    pub size: u64,
    /// "PDF document", "ZIP archive"…; `None` when it was too big to read.
    pub kind: Option<String>,
    pub facts: Vec<sniff::Fact>,
    /// Its first [`HEX_BYTES`] bytes.
    pub head: Vec<u8>,
    pub text: Option<String>,
    pub members: Option<Vec<zip::Member>>,
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
    /// The text of both sides diffed, when both (or the one there) have text.
    pub text: Option<Vec<(Edit, String)>>,
    /// The members of both archives that differ.
    pub members: Option<Vec<MemberChange>>,
    /// Why a side couldn't be read.
    pub error: Option<String>,
}

/// Whether git's patch says the file is binary.
pub(super) fn is_binary_patch(text: &str) -> bool {
    text.lines()
        .take(12)
        .any(|l| l.starts_with("Binary files ") || l == "GIT binary patch")
}

/// One version of `path`: at `rev` (`Some("")` the index, `None` the working tree).
fn read_side(repo: &Path, rev: Option<&str>, path: &str) -> Result<Option<Side>, String> {
    let size = match rev {
        None => match crate::remote::read_within(repo, path, MAX_READ) {
            Some(read) => {
                return match read {
                    Ok(Some(bytes)) => Ok(Some(side(path, bytes.len() as u64, &bytes))),
                    Ok(None) => Ok(Some(too_big(None))),
                    Err(e) => Err(format!("{path}: {e}")),
                };
            }
            None => match std::fs::metadata(repo.join(path)) {
                Ok(m) => m.len(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(format!("{path}: {e}")),
            },
        },
        Some(r) => {
            let spec = if r.is_empty() {
                format!(":0:{path}")
            } else {
                format!("{r}:{path}")
            };
            match git::run(Some(repo), &["cat-file", "-s", &spec]) {
                Ok(n) => n.trim().parse().unwrap_or(0),
                // Not at that revision: the side without the file.
                Err(_) => return Ok(None),
            }
        }
    };
    if size > MAX_READ {
        return Ok(Some(too_big(Some(size))));
    }
    let bytes = git::file_at(repo, rev, path)?;
    Ok(Some(side(path, size, &bytes)))
}

fn too_big(size: Option<u64>) -> Side {
    Side {
        size: size.unwrap_or(MAX_READ + 1),
        kind: None,
        facts: Vec::new(),
        head: Vec::new(),
        text: None,
        members: None,
    }
}

fn side(path: &str, size: u64, bytes: &[u8]) -> Side {
    let (kind, facts) = sniff::describe(path, bytes);
    Side {
        size,
        kind: Some(kind),
        facts,
        head: bytes[..bytes.len().min(HEX_BYTES)].to_vec(),
        text: extract::text_of(path, bytes),
        members: zip::is_zip(bytes).then(|| zip::members(bytes)).flatten(),
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
    pair(old.ok().flatten(), new.ok().flatten(), error)
}

/// The pair, with what changed between its sides worked out.
pub(super) fn pair(old: Option<Side>, new: Option<Side>, error: Option<String>) -> BinaryPair {
    fn text_of(s: &Option<Side>) -> Option<&str> {
        s.as_ref().and_then(|s| s.text.as_deref())
    }
    fn members_of(s: &Option<Side>) -> Option<&Vec<zip::Member>> {
        s.as_ref().and_then(|s| s.members.as_ref())
    }
    let text = match (text_of(&old), text_of(&new)) {
        (None, None) => None,
        (a, b) => {
            let (a, b): (Vec<&str>, Vec<&str>) = (
                a.map(|t| t.lines().collect()).unwrap_or_default(),
                b.map(|t| t.lines().collect()).unwrap_or_default(),
            );
            Some(
                diff_lines(&a, &b)
                    .into_iter()
                    .map(|(e, l)| (e, l.to_owned()))
                    .collect(),
            )
        }
    };
    let members = match (members_of(&old), members_of(&new)) {
        (None, None) => None,
        (a, b) => Some(member_changes(
            a.map_or(&[], Vec::as_slice),
            b.map_or(&[], Vec::as_slice),
        )),
    };
    BinaryPair {
        old,
        new,
        text,
        members,
        error,
    }
}

/// The members added, removed or changed (by size or checksum), by name.
pub(super) fn member_changes(old: &[zip::Member], new: &[zip::Member]) -> Vec<MemberChange> {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<&str, (Option<&zip::Member>, Option<&zip::Member>)> = BTreeMap::new();
    for m in old {
        by_name.entry(&m.name).or_default().0 = Some(m);
    }
    for m in new {
        by_name.entry(&m.name).or_default().1 = Some(m);
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

/// The `textconv` program git turns `path` into text with for diffs, if its `diff` attribute
/// names a driver that has one: the diff shown is then that text, not the file.
pub fn textconv_for(repo: &Path, path: &str) -> Option<String> {
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
