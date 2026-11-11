//! A file's patch in a commit, made in-process as `git show --diff-merges=first-parent --root
//! -M <hash> -- <path>` prints it (with gitcraft's `--no-color --no-ext-diff --src-prefix=a/
//! --dst-prefix=b/`): a diff opened starts no git process. Whatever git would print otherwise
//! (a diff driver or textconv, submodules, `-w`, patience, settings changing how hunks are cut)
//! is an error, and the caller asks git instead.

use std::path::Path;

use gix::bstr::{BStr, ByteSlice};
use gix::diff::blob::{Algorithm, Diff, InternedInput, sources::byte_lines};
use gix::object::tree::diff::ChangeDetached;
use gix::prelude::ObjectIdExt;

use super::super::{DiffAlgorithm, DiffOptions};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The patch of `file` in commit `hash`, as `file_diff` asks git for it.
pub(in crate::git) fn file_patch(
    path: &Path,
    hash: &str,
    file: &str,
    opts: DiffOptions,
) -> Result<String, String> {
    if opts.ignore_ws {
        return Err("diff options".into());
    }
    let repo = super::native::open(path)?;
    super::commits::plain_history(&repo)?;
    let settings = Settings::read(&repo, opts)?;
    let changes = super::commits::changes(&repo, hash)?;
    if !super::commits::paired_as_git(&changes) {
        return Err("listed by git".into());
    }
    let change = changes
        .iter()
        .find(|c| c.location() == file.as_bytes().as_bstr())
        .ok_or("not in the commit")?;
    let mut cache = repo.diff_resource_cache_for_tree_diff().map_err(err)?;
    let patch = patch(&repo, change, &settings, &mut cache)?;
    Ok(String::from_utf8_lossy(&patch).into_owned())
}

/// The bytes of `file` in `rev` (a full commit id, or `HEAD`), as `git show <rev>:<file>`
/// prints a file or a link.
pub(in crate::git) fn file_at(path: &Path, rev: &str, file: &str) -> Result<Vec<u8>, String> {
    let repo = super::native::open(path)?;
    super::commits::plain_history(&repo)?;
    let commit = if rev == "HEAD" {
        repo.head_commit().map_err(err)?
    } else {
        let id = gix::ObjectId::from_hex(rev.as_bytes()).map_err(err)?;
        repo.find_commit(id).map_err(err)?
    };
    let entry = commit
        .tree()
        .map_err(err)?
        .lookup_entry_by_path(file)
        .map_err(err)?
        .ok_or("not in the commit")?;
    if !(entry.mode().is_blob() || entry.mode().is_link()) {
        return Err("not a plain file".into());
    }
    Ok(entry.object().map_err(err)?.detach().data)
}

/// The bytes `file` had before commit `hash` (in its first parent, under its old name when
/// renamed) and in it; none for the side it isn't on.
pub(in crate::git) fn commit_file_sides(
    path: &Path,
    hash: &str,
    file: &str,
) -> Result<super::super::FileSides, String> {
    let repo = super::native::open(path)?;
    super::commits::plain_history(&repo)?;
    let changes = super::commits::changes(&repo, hash)?;
    if !super::commits::paired_as_git(&changes) {
        return Err("listed by git".into());
    }
    let change = changes
        .iter()
        .find(|c| c.location() == file.as_bytes().as_bstr())
        .ok_or("not in the commit")?;
    let (old, new) = sides(change)?;
    let data = |side: &Side<'_>| -> Result<Option<Vec<u8>>, String> {
        if side.mode.is_none() {
            return Ok(None);
        }
        Ok(Some(repo.find_object(side.id).map_err(err)?.detach().data))
    };
    Ok((data(&old)?, data(&new)?))
}

/// The textconv program of `file`'s diff driver, as `check-attr diff` and `config diff.<driver>
/// .textconv` find it (attributes of the working tree, then of the index): none without one.
pub(in crate::git) fn textconv_of(path: &Path, file: &str) -> Result<Option<String>, String> {
    let repo = super::native::open(path)?;
    let roots = gix::diff::blob::pipeline::WorktreeRoots {
        old_root: None,
        new_root: repo.workdir().map(Path::to_owned),
    };
    let mut cache = repo
        .diff_resource_cache(gix::diff::blob::pipeline::Mode::ToGit, roots)
        .map_err(err)?;
    let Some(driver) = diff_driver(&repo, &mut cache, file.as_bytes().as_bstr())? else {
        return Ok(None);
    };
    let config = repo.config_snapshot();
    let textconv = config
        .plumbing()
        .string(format!("diff.{driver}.textconv").as_str())
        .map(|v| v.to_str_lossy().trim().to_owned())
        .filter(|v| !v.is_empty());
    Ok(textconv)
}

/// A commit's author, email and message as `%an`, `%ae` and `%B` give them, and its note in
/// each of `notes` (as `%N` gives one: each line ended), none where it has none.
pub(in crate::git) fn authorship(
    path: &Path,
    hash: &str,
    notes: &[&str],
) -> Result<super::super::Authorship, String> {
    let repo = super::native::open(path)?;
    super::commits::plain_history(&repo)?;
    let id = gix::ObjectId::from_hex(hash.as_bytes()).map_err(err)?;
    let commit = repo.find_commit(id).map_err(err)?;
    let decoded = commit.decode().map_err(err)?;
    if decoded
        .encoding
        .is_some_and(|e| !e.eq_ignore_ascii_case(b"utf-8") && !e.eq_ignore_ascii_case(b"utf8"))
    {
        return Err("commit encoding".into());
    }
    let author = decoded.author().map_err(err)?;
    let text = |b: &BStr| b.to_str().map(str::to_owned).map_err(err);
    let notes = notes
        .iter()
        .map(|notes| note(&repo, notes, id))
        .collect::<Result<_, _>>()?;
    Ok((
        text(author.name)?,
        text(author.email)?,
        text(decoded.message)?,
        notes,
    ))
}

/// The note of commit `id` in the notes ref `notes`, its lines each ended with a newline.
fn note(repo: &gix::Repository, notes: &str, id: gix::ObjectId) -> Result<Option<String>, String> {
    let Some(mut reference) = repo.try_find_reference(notes).map_err(err)? else {
        return Ok(None);
    };
    let tree = reference
        .peel_to_commit()
        .map_err(err)?
        .tree()
        .map_err(err)?;
    let hex = id.to_string();
    // As many levels of two-digit folders as the notes have been fanned out to.
    for depth in 0..hex.len() / 2 {
        let mut path = String::new();
        for level in 0..depth {
            path.push_str(&hex[level * 2..level * 2 + 2]);
            path.push('/');
        }
        path.push_str(&hex[depth * 2..]);
        let Some(entry) = tree.lookup_entry_by_path(&path).map_err(err)? else {
            continue;
        };
        if !entry.mode().is_blob() {
            return Err("note is not a blob".into());
        }
        let data = entry.object().map_err(err)?.detach().data;
        let text = String::from_utf8(data).map_err(err)?;
        let mut lines = String::with_capacity(text.len() + 1);
        for line in text.split_inclusive('\n') {
            lines.push_str(line);
            if !line.ends_with('\n') {
                lines.push('\n');
            }
        }
        return Ok(Some(lines));
    }
    Ok(None)
}

/// What of the configuration changes how git prints a patch.
struct Settings {
    algorithm: Algorithm,
    context: usize,
    /// `core.quotePath`: names with bytes beyond ASCII are quoted.
    quote_fully: bool,
}

impl Settings {
    fn read(repo: &gix::Repository, opts: DiffOptions) -> Result<Self, String> {
        let config = repo.config_snapshot();
        let config = config.plumbing();
        let configured = config.string("diff.algorithm");
        let algorithm = match opts.algorithm {
            Some(DiffAlgorithm::Myers) => Algorithm::Myers,
            Some(DiffAlgorithm::Minimal) => Algorithm::MyersMinimal,
            Some(DiffAlgorithm::Histogram) => Algorithm::Histogram,
            Some(DiffAlgorithm::Patience) => return Err("diff options".into()),
            None => match configured.as_deref().map(|v| v.to_ascii_lowercase()) {
                None => Algorithm::Myers,
                Some(v) if v == b"myers" || v == b"default" => Algorithm::Myers,
                Some(v) if v == b"minimal" => Algorithm::MyersMinimal,
                Some(v) if v == b"histogram" => Algorithm::Histogram,
                Some(_) => return Err("diff options".into()),
            },
        };
        let flag = |key: &str| config.boolean(key).map_err(err);
        let number = |key: &str| config.integer(key).map_err(err);
        if flag("diff.indentHeuristic")? == Some(false)
            || flag("diff.suppressBlankEmpty")? == Some(true)
            || number("diff.interHunkContext")?.is_some_and(|n| n != 0)
        {
            return Err("diff options".into());
        }
        // As `keep_context` asks git: three lines when the config says none.
        let context = match number("diff.context")? {
            _ if opts.whole_file => WHOLE_FILE,
            None | Some(0) => 3,
            Some(n) => usize::try_from(n).map_err(|_| "diff options")?,
        };
        Ok(Self {
            algorithm,
            context,
            quote_fully: flag("core.quotePath")?.unwrap_or(true),
        })
    }
}

/// One side of a change: its path, mode and blob (none for the side a file was added or
/// deleted on).
struct Side<'a> {
    path: &'a BStr,
    mode: Option<gix::object::tree::EntryMode>,
    id: gix::ObjectId,
}

fn sides(change: &ChangeDetached) -> Result<(Side<'_>, Side<'_>), String> {
    use ChangeDetached as C;
    let null = |id: &gix::ObjectId| gix::ObjectId::null(id.kind());
    Ok(match change {
        C::Addition {
            location,
            entry_mode,
            id,
            ..
        } => (
            Side {
                path: location.as_bstr(),
                mode: None,
                id: null(id),
            },
            Side {
                path: location.as_bstr(),
                mode: Some(*entry_mode),
                id: *id,
            },
        ),
        C::Deletion {
            location,
            entry_mode,
            id,
            ..
        } => (
            Side {
                path: location.as_bstr(),
                mode: Some(*entry_mode),
                id: *id,
            },
            Side {
                path: location.as_bstr(),
                mode: None,
                id: null(id),
            },
        ),
        C::Modification {
            location,
            previous_entry_mode,
            previous_id,
            entry_mode,
            id,
        } => (
            Side {
                path: location.as_bstr(),
                mode: Some(*previous_entry_mode),
                id: *previous_id,
            },
            Side {
                path: location.as_bstr(),
                mode: Some(*entry_mode),
                id: *id,
            },
        ),
        C::Rewrite {
            source_location,
            source_entry_mode,
            source_id,
            entry_mode,
            id,
            location,
            diff: None,
            copy: false,
            ..
        } => (
            Side {
                path: source_location.as_bstr(),
                mode: Some(*source_entry_mode),
                id: *source_id,
            },
            Side {
                path: location.as_bstr(),
                mode: Some(*entry_mode),
                id: *id,
            },
        ),
        C::Rewrite { .. } => return Err("listed by git".into()),
    })
}

#[cfg(test)]
mod tests;

mod render;
use render::WHOLE_FILE;
use render::diff_driver;
use render::patch;
