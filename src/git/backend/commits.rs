//! A commit's header and the files it changed, read in-process as `git show` gives them: a
//! click or a read-ahead starts no git process (each about a tenth of a second on Windows).
//! Whatever git would read differently (rewritten history, re-encoded messages, names that
//! aren't UTF-8) is an error, and the caller asks git instead. Line counts ignore textconv
//! drivers, as `--numstat` does.

use std::path::Path;

use gix::bstr::ByteSlice;
use gix::object::tree::EntryKind;
use gix::object::tree::diff::ChangeDetached;
use gix::prelude::TreeDiffChangeExt;

use super::super::{ChangedFile, CommitDetail};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn text(bytes: &gix::bstr::BStr) -> Result<String, String> {
    bytes.to_str().map(str::to_owned).map_err(err)
}

/// Grafts, a shallow clone and replaced objects change what git reads as a commit's parents.
fn plain_history(repo: &gix::Repository) -> Result<(), String> {
    if repo.common_dir().join("info/grafts").exists()
        || repo.common_dir().join("shallow").exists()
        || repo
            .references()
            .map_err(err)?
            .prefixed("refs/replace/")
            .map_err(err)?
            .next()
            .is_some()
    {
        return Err("history-overlays".into());
    }
    Ok(())
}

fn commit_id(hash: &str) -> Result<gix::ObjectId, String> {
    gix::ObjectId::from_hex(hash.as_bytes()).map_err(err)
}

/// The files of each of `hashes`, as `git show --diff-merges=first-parent --root -M` lists
/// them with `--name-status` and `--numstat`; none for a commit only git lists faithfully.
pub(in crate::git) fn files(
    path: &Path,
    hashes: &[String],
) -> Result<Vec<(String, Files)>, String> {
    let repo = super::native::open(path)?;
    plain_history(&repo)?;
    files_in(&repo, hashes)
}

/// A commit's files, or none when git is to list them: a file may have been renamed with
/// changes (git and gitoxide weigh similarity differently, so they may pair other files),
/// copied, or have become another kind of thing (a file a link).
pub(in crate::git) type Files = Option<Vec<ChangedFile>>;

fn files_in(repo: &gix::Repository, hashes: &[String]) -> Result<Vec<(String, Files)>, String> {
    let mut cache = repo.diff_resource_cache_for_tree_diff().map_err(err)?;
    cache.options.skip_internal_diff_if_external_is_configured = false;
    let drivers: Vec<Driver> = cache
        .filter
        .drivers()
        .iter()
        .map(|d| Driver {
            textconv: d.binary_to_text_command.is_some(),
            binary: d.is_binary,
        })
        .collect();
    let mut options = gix::diff::Options::default();
    // `-M`: renames at git's default similarity, no copies, whatever the config says.
    options
        .track_path()
        .track_rewrites(Some(gix::diff::Rewrites::default()));
    let empty = repo.empty_tree();
    hashes
        .iter()
        .map(|hash| {
            let commit = repo.find_commit(commit_id(hash)?).map_err(err)?;
            let tree = commit.tree().map_err(err)?;
            let parent = match commit.parent_ids().next() {
                Some(id) => id
                    .object()
                    .map_err(err)?
                    .try_into_commit()
                    .map_err(err)?
                    .tree()
                    .map_err(err)?,
                None => empty.clone(),
            };
            let changes = repo
                .diff_tree_to_tree(&parent, &tree, options)
                .map_err(err)?;
            let files = listed(repo, &changes, &mut cache, &drivers)?;
            cache.clear_resource_cache_keep_allocation();
            Ok((hash.clone(), files))
        })
        .collect()
}

fn listed(
    repo: &gix::Repository,
    changes: &[ChangeDetached],
    cache: &mut gix::diff::blob::Platform,
    drivers: &[Driver],
) -> Result<Files, String> {
    let file = |mode: &gix::object::tree::EntryMode| !mode.is_tree() && !mode.is_commit();
    let (mut added, mut deleted) = (false, false);
    for change in changes {
        match change {
            ChangeDetached::Addition { entry_mode, .. } => added |= file(entry_mode),
            ChangeDetached::Deletion { entry_mode, .. } => deleted |= file(entry_mode),
            // Moved as it was: git pairs those the same.
            ChangeDetached::Rewrite {
                diff: None,
                copy: false,
                ..
            } => {}
            ChangeDetached::Rewrite { .. } => return Ok(None),
            ChangeDetached::Modification { .. } => {}
        }
    }
    // What was added and deleted git may see as renamed, and the other way round.
    if added && deleted {
        return Ok(None);
    }
    let mut files = Vec::new();
    for change in changes {
        match changed_file(repo, change, cache, drivers)? {
            Listed::File(file) => files.push(file),
            Listed::Folder => {}
            Listed::ByGit => return Ok(None),
        }
    }
    // In git's order: by path (a rename where it ends up).
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Some(files))
}

/// What a diff driver (`diff=<name>` in `.gitattributes`) says of a file being binary.
struct Driver {
    /// It turns the file to text for showing it: `--numstat` still counts what is stored.
    textconv: bool,
    binary: Option<bool>,
}

enum Listed {
    File(ChangedFile),
    /// Not listed, only what is in it.
    Folder,
    /// Git counts it its own way.
    ByGit,
}

/// A file kind as `--name-status` tells them apart: an executable file is still a file.
fn kind(mode: gix::object::tree::EntryMode) -> EntryKind {
    match mode.kind() {
        EntryKind::BlobExecutable => EntryKind::Blob,
        other => other,
    }
}

fn changed_file(
    repo: &gix::Repository,
    change: &ChangeDetached,
    cache: &mut gix::diff::blob::Platform,
    drivers: &[Driver],
) -> Result<Listed, String> {
    use ChangeDetached as C;
    let (status, path, old_path, old_mode, new_mode) = match change {
        C::Addition {
            location,
            entry_mode,
            ..
        } => ('A', location, None, None, Some(*entry_mode)),
        C::Deletion {
            location,
            entry_mode,
            ..
        } => ('D', location, None, Some(*entry_mode), None),
        C::Modification {
            location,
            previous_entry_mode,
            entry_mode,
            ..
        } => {
            if kind(*previous_entry_mode) != kind(*entry_mode) {
                return Ok(Listed::ByGit);
            }
            (
                'M',
                location,
                None,
                Some(*previous_entry_mode),
                Some(*entry_mode),
            )
        }
        C::Rewrite {
            source_location,
            source_entry_mode,
            entry_mode,
            location,
            ..
        } => (
            'R',
            location,
            Some(source_location),
            Some(*source_entry_mode),
            Some(*entry_mode),
        ),
    };
    if old_mode.or(new_mode).is_some_and(|m| m.is_tree()) {
        return Ok(Listed::Folder);
    }
    let submodule = |m: Option<gix::object::tree::EntryMode>| m.is_some_and(|m| m.is_commit());
    let (added, removed) = if submodule(old_mode) || submodule(new_mode) {
        // Git counts a submodule's change as its "Subproject commit" line.
        (
            Some(u32::from(new_mode.is_some())),
            Some(u32::from(old_mode.is_some())),
        )
    } else {
        line_counts(repo, change, cache, drivers)?
    };
    Ok(Listed::File(ChangedFile {
        status,
        path: text(path.as_bstr())?,
        old_path: old_path.map(|p| text(p.as_bstr())).transpose()?,
        added,
        removed,
    }))
}

/// Lines added and removed as `--numstat` counts them, or none for a binary file. A line's
/// ending is part of it: a last line given its newline, or a file turned to CRLF, counts.
fn line_counts(
    repo: &gix::Repository,
    change: &ChangeDetached,
    cache: &mut gix::diff::blob::Platform,
    drivers: &[Driver],
) -> Result<(Option<u32>, Option<u32>), String> {
    use gix::diff::blob::platform::prepare_diff::Operation;
    let platform = change.attach(repo, repo).diff(cache).map_err(err)?;
    let prepared = platform.resource_cache.prepare_diff().map_err(err)?;
    // Gitoxide takes a file with a textconv driver for text; git's counts don't convert it,
    // and decide on what is stored: the driver's `binary`, or a NUL among the first 8000 bytes.
    let binary = |resource: &gix::diff::blob::platform::Resource<'_>| {
        resource
            .driver_index
            .and_then(|i| drivers.get(i))
            .filter(|driver| driver.textconv)
            .is_some_and(|driver| {
                driver.binary.unwrap_or_else(|| {
                    let data = resource.data.as_slice().unwrap_or_default();
                    data[..data.len().min(8000)].contains(&0)
                })
            })
    };
    match prepared.operation {
        Operation::InternalDiff { .. } if binary(&prepared.old) || binary(&prepared.new) => {
            Ok((None, None))
        }
        Operation::InternalDiff { algorithm } => {
            let input = gix::diff::blob::InternedInput::new(
                prepared.old.intern_source(),
                prepared.new.intern_source(),
            );
            let diff = gix::diff::blob::Diff::compute(algorithm, &input);
            Ok((Some(diff.count_additions()), Some(diff.count_removals())))
        }
        Operation::SourceOrDestinationIsBinary => Ok((None, None)),
        Operation::ExternalCommand { .. } => Err("external diff".into()),
    }
}

/// `hash`'s detail as `git show` gives it (`--date=format-local:%Y-%m-%d %H:%M:%S`), whether
/// HEAD contains it, and whether a merge came after it on the way to HEAD.
pub(in crate::git) fn detail(path: &Path, hash: &str) -> Result<CommitDetail, String> {
    let repo = super::native::open(path)?;
    plain_history(&repo)?;
    let id = commit_id(hash)?;
    let commit = repo.find_commit(id).map_err(err)?;
    let decoded = commit.decode().map_err(err)?;
    // Git re-encodes a message stored in another encoding.
    if decoded
        .encoding
        .is_some_and(|e| !e.eq_ignore_ascii_case(b"utf-8") && !e.eq_ignore_ascii_case(b"utf8"))
    {
        return Err("commit encoding".into());
    }
    let author = decoded.author().map_err(err)?;
    let committer = decoded.committer().map_err(err)?;
    let parents = decoded.parents().map(|p| p.to_string()).collect();
    let message = text(decoded.message)?.trim_end().to_owned();
    let (author_name, email, author_date) = (
        text(author.name)?,
        text(author.email)?,
        local_time(author.time().map_err(err)?.seconds)?,
    );
    let (committer_name, commit_date) = (
        text(committer.name)?,
        local_time(committer.time().map_err(err)?.seconds)?,
    );
    let files = match files_in(&repo, &[hash.to_owned()])?.pop() {
        Some((_, Some(files))) => files,
        _ => crate::git::diff::commit_files_by_git(path, hash)?,
    };
    let head = repo.head().map_err(err)?;
    let head = if head.is_unborn() {
        None
    } else {
        head.id().map(|id| id.detach())
    };
    let in_head = match head {
        Some(head) if head == id => true,
        Some(head) => repo
            .merge_base(id, head)
            .map_err(err)?
            .is_some_and(|base| base == id),
        None => false,
    };
    let merges_after = match head {
        Some(head) if in_head && head != id => {
            let mut merges = false;
            for info in repo.rev_walk([head]).with_hidden([id]).all().map_err(err)? {
                if info.map_err(err)?.parent_ids.len() > 1 {
                    merges = true;
                    break;
                }
            }
            merges
        }
        _ => false,
    };
    Ok(CommitDetail {
        hash: hash.to_owned(),
        parents,
        author: author_name,
        email,
        author_date,
        committer: committer_name,
        commit_date,
        message,
        files,
        in_head,
        merges_after,
    })
}

/// `diff.context`, as `git config --type=int` reads it: none when it isn't set.
pub(in crate::git) fn diff_context(path: &Path) -> Result<Option<i64>, String> {
    let repo = super::native::open(path)?;
    repo.config_snapshot()
        .plumbing()
        .integer("diff.context")
        .map_err(err)
}

/// `seconds` since the epoch in this machine's time zone, as `format-local:%Y-%m-%d %H:%M:%S`.
fn local_time(seconds: i64) -> Result<String, String> {
    let at = jiff::Timestamp::from_second(seconds).map_err(err)?;
    Ok(at
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string())
}

#[cfg(test)]
mod tests;
