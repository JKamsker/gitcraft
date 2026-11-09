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

/// Context lines when the whole file is asked for: more than any file has.
const WHOLE_FILE: usize = usize::MAX / 4;

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

/// A file, as git tells them apart in a patch: a link's target is diffed as text, but a file
/// that became a link (or a folder, a submodule) git prints its own way.
fn plain(mode: Option<gix::object::tree::EntryMode>) -> bool {
    mode.is_none_or(|m| m.is_blob() || m.is_link())
}

fn patch(
    repo: &gix::Repository,
    change: &ChangeDetached,
    settings: &Settings,
    cache: &mut gix::diff::blob::Platform,
) -> Result<Vec<u8>, String> {
    let (old, new) = sides(change)?;
    if !plain(old.mode)
        || !plain(new.mode)
        || old
            .mode
            .zip(new.mode)
            .is_some_and(|(o, n)| o.is_link() != n.is_link())
    {
        return Err("not a plain file".into());
    }
    let quote = |prefix: &str, path: &BStr| quoted(prefix, path, settings.quote_fully);
    let (a, b) = (quote("a/", old.path), quote("b/", new.path));
    let mut out = Vec::new();
    out.extend_from_slice(b"diff --git ");
    out.extend_from_slice(&a);
    out.push(b' ');
    out.extend_from_slice(&b);
    out.push(b'\n');
    let octal = |m: gix::object::tree::EntryMode| format!("{:06o}", m.value());
    let renamed = old.path != new.path;
    let mut must_show = renamed;
    match (old.mode, new.mode) {
        (None, Some(mode)) => {
            out.extend_from_slice(format!("new file mode {}\n", octal(mode)).as_bytes());
            must_show = true;
        }
        (Some(mode), None) => {
            out.extend_from_slice(format!("deleted file mode {}\n", octal(mode)).as_bytes());
            must_show = true;
        }
        (Some(before), Some(after)) if before != after => {
            out.extend_from_slice(format!("old mode {}\n", octal(before)).as_bytes());
            out.extend_from_slice(format!("new mode {}\n", octal(after)).as_bytes());
            must_show = true;
        }
        _ => {}
    }
    if renamed {
        out.extend_from_slice(b"similarity index 100%\nrename from ");
        out.extend_from_slice(&quoted("", old.path, settings.quote_fully));
        out.extend_from_slice(b"\nrename to ");
        out.extend_from_slice(&quoted("", new.path, settings.quote_fully));
        out.push(b'\n');
    }
    if old.id != new.id {
        let (before, after) = abbreviated(repo, old.id, new.id)?;
        out.extend_from_slice(format!("index {before}..{after}").as_bytes());
        if let (Some(before), Some(after)) = (old.mode, new.mode)
            && before == after
        {
            out.extend_from_slice(format!(" {}", octal(before)).as_bytes());
        }
        out.push(b'\n');
    }
    let header = out;
    let data = |side: &Side<'_>| -> Result<Vec<u8>, String> {
        if side.mode.is_none() {
            return Ok(Vec::new());
        }
        Ok(repo.find_object(side.id).map_err(err)?.detach().data)
    };
    let (before, after) = (data(&old)?, data(&new)?);
    let label = |side: &Side<'_>, quoted: &[u8]| -> Vec<u8> {
        if side.mode.is_some() {
            quoted.to_vec()
        } else {
            b"/dev/null".to_vec()
        }
    };
    let (from, to) = (label(&old, &a), label(&new, &b));
    let binary = match diff_attribute(repo, cache, new.path)? {
        Attribute::Unset => true,
        Attribute::Set => false,
        Attribute::Unspecified => {
            let nul = |data: &[u8]| data[..data.len().min(8000)].contains(&0);
            nul(&before) || nul(&after)
        }
        Attribute::Driver => return Err("diff driver".into()),
    };
    let mut out = header;
    if binary {
        if before == after {
            return Ok(if must_show { out } else { Vec::new() });
        }
        out.extend_from_slice(b"Binary files ");
        out.extend_from_slice(&from);
        out.extend_from_slice(b" and ");
        out.extend_from_slice(&to);
        out.extend_from_slice(b" differ\n");
        return Ok(out);
    }
    let hunks = hunks(&before, &after, settings);
    if hunks.is_empty() {
        return Ok(if must_show { out } else { Vec::new() });
    }
    // A name with a space ends with a tab, so that a patch reader knows where it ends.
    let tab = |label: &[u8]| if label.contains(&b' ') { "\t" } else { "" };
    out.extend_from_slice(b"--- ");
    out.extend_from_slice(&from);
    out.extend_from_slice(tab(&from).as_bytes());
    out.extend_from_slice(b"\n+++ ");
    out.extend_from_slice(&to);
    out.extend_from_slice(tab(&to).as_bytes());
    out.push(b'\n');
    out.extend_from_slice(&hunks);
    Ok(out)
}

/// The ids on an `index` line, each as short as `core.abbrev` and uniqueness allow; a side
/// that isn't there is zeros, as long as the other's.
fn abbreviated(
    repo: &gix::Repository,
    old: gix::ObjectId,
    new: gix::ObjectId,
) -> Result<(String, String), String> {
    let short = |id: gix::ObjectId| -> Result<Option<String>, String> {
        if id.is_null() {
            return Ok(None);
        }
        Ok(Some(id.attach(repo).shorten().map_err(err)?.to_string()))
    };
    let (before, after) = (short(old)?, short(new)?);
    let zeros = |other: &Option<String>| "0".repeat(other.as_ref().map_or(7, String::len));
    Ok((
        before.clone().unwrap_or_else(|| zeros(&after)),
        after.unwrap_or_else(|| zeros(&before)),
    ))
}

/// What `.gitattributes` say of diffing a file.
enum Attribute {
    /// `-diff` (or `binary`): never as text.
    Unset,
    /// `diff`: always as text.
    Set,
    Unspecified,
    /// `diff=<driver>`: its hunk headers, its textconv, its binary; git's to print.
    Driver,
}

fn diff_attribute(
    repo: &gix::Repository,
    cache: &mut gix::diff::blob::Platform,
    path: &BStr,
) -> Result<Attribute, String> {
    let mut outcome = gix::attrs::search::Outcome::default();
    outcome.initialize_with_selection(&Default::default(), Some("diff"));
    let entry = cache
        .attr_stack
        .at_entry(path, None, &repo.objects)
        .map_err(err)?;
    entry.matching_attributes(&mut outcome);
    let state = outcome
        .iter_selected()
        .next()
        .map(|m| m.assignment.state.to_owned());
    Ok(match state {
        Some(gix::attrs::State::Unset) => Attribute::Unset,
        Some(gix::attrs::State::Set) => Attribute::Set,
        Some(gix::attrs::State::Value(_)) => Attribute::Driver,
        Some(gix::attrs::State::Unspecified) | None => Attribute::Unspecified,
    })
}

/// `prefix` and `path` as git writes a name in a patch: in quotes, with C escapes, when it has
/// a control character, a quote or backslash, or (with `core.quotePath`) a byte beyond ASCII.
fn quoted(prefix: &str, path: &BStr, fully: bool) -> Vec<u8> {
    let must = |b: u8| b < 0x20 || b == b'"' || b == b'\\' || b == 0x7f || (fully && b >= 0x80);
    let name: Vec<u8> = prefix.bytes().chain(path.iter().copied()).collect();
    if !name.iter().any(|&b| must(b)) {
        return name;
    }
    let mut out = vec![b'"'];
    for b in name {
        let escape = match b {
            0x07 => Some(b'a'),
            0x08 => Some(b'b'),
            b'\t' => Some(b't'),
            b'\n' => Some(b'n'),
            0x0b => Some(b'v'),
            0x0c => Some(b'f'),
            b'\r' => Some(b'r'),
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            _ => None,
        };
        match escape {
            Some(letter) => out.extend_from_slice(&[b'\\', letter]),
            None if must(b) => out.extend_from_slice(format!("\\{b:03o}").as_bytes()),
            None => out.push(b),
        }
    }
    out.push(b'"');
    out
}

/// The hunks of a text diff, cut and headed as xdiff does (`xemit.c`): changes no more than
/// twice the context apart share a hunk; each header names the nearest line above it that
/// starts with a letter, `_` or `$` (searched no further up than the previous hunk).
fn hunks(before: &[u8], after: &[u8], settings: &Settings) -> Vec<u8> {
    let input = InternedInput::new(byte_lines(before), byte_lines(after));
    let mut diff = Diff::compute(settings.algorithm, &input);
    diff.postprocess_lines(&input);
    let changes: Vec<_> = diff.hunks().collect();
    let line = |token: &gix::diff::blob::Token| input.interner[*token];
    let old: Vec<&[u8]> = input.before.iter().map(line).collect();
    let new: Vec<&[u8]> = input.after.iter().map(line).collect();
    let context = settings.context;
    let mut out = Vec::new();
    let mut func: Vec<u8> = Vec::new();
    let mut searched_to: isize = -1;
    let mut i = 0;
    while i < changes.len() {
        let mut last = i;
        while last + 1 < changes.len()
            && (changes[last + 1].before.start - changes[last].before.end) as usize
                <= context.saturating_mul(2)
        {
            last += 1;
        }
        let (first, end) = (&changes[i], &changes[last]);
        let s1 = (first.before.start as usize).saturating_sub(context);
        let s2 = first.after.start as usize - (first.before.start as usize - s1);
        let e1 = (end.before.end as usize)
            .saturating_add(context)
            .min(old.len());
        let e2 = end.after.end as usize + (e1 - end.before.end as usize);
        let mut at = s1 as isize - 1;
        while at > searched_to && at >= 0 {
            if let Some(name) = function_name(old[at as usize]) {
                func = name.to_vec();
                break;
            }
            at -= 1;
        }
        searched_to = s1 as isize - 1;
        let range = |start: usize, count: usize| {
            let start = if count == 0 { start } else { start + 1 };
            if count == 1 {
                start.to_string()
            } else {
                format!("{start},{count}")
            }
        };
        out.extend_from_slice(
            format!("@@ -{} +{} @@", range(s1, e1 - s1), range(s2, e2 - s2)).as_bytes(),
        );
        if !func.is_empty() {
            out.push(b' ');
            out.extend_from_slice(&func);
        }
        out.push(b'\n');
        let mut emit = |sign: u8, text: &[u8]| {
            out.push(sign);
            out.extend_from_slice(text);
            if !text.ends_with(b"\n") {
                out.extend_from_slice(b"\n\\ No newline at end of file\n");
            }
        };
        let mut k = s1;
        for change in &changes[i..=last] {
            for text in &old[k..change.before.start as usize] {
                emit(b' ', text);
            }
            for text in &old[change.before.start as usize..change.before.end as usize] {
                emit(b'-', text);
            }
            for text in &new[change.after.start as usize..change.after.end as usize] {
                emit(b'+', text);
            }
            k = change.before.end as usize;
        }
        for text in &old[k..e1] {
            emit(b' ', text);
        }
        i = last + 1;
    }
    out
}

/// xdiff's default function line (`def_ff`): one starting with a letter, `_` or `$`, at most
/// 80 bytes of it, without the whitespace it ends with.
fn function_name(line: &[u8]) -> Option<&[u8]> {
    let first = *line.first()?;
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return None;
    }
    let mut name = &line[..line.len().min(80)];
    while let Some((&last, rest)) = name.split_last() {
        if !matches!(last, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
            break;
        }
        name = rest;
    }
    Some(name)
}

#[cfg(test)]
mod tests;
