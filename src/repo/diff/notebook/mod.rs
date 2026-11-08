//! Jupyter notebooks in a diff: cell by cell rather than as JSON, the source of each changed
//! cell diffed, its outputs (pictures included) shown before and after, execution counts and
//! metadata folded away; a switch back to the JSON, which stages a line at a time as before;
//! and "Strip outputs when committing" for the repository. Nothing in a notebook is run.

mod compare;
mod parse;
mod search;
mod view;
pub(super) use search::document as search_document;

use std::path::Path;

pub(super) use view::notebook_bar;

use super::DiffKey;
pub use compare::NotebookDiff;

/// A notebook's diff, how the repository strips outputs when committing, and whether the
/// working-tree side was compared as it would be staged (outputs stripped).
pub struct NotebookData {
    pub diff: NotebookDiff,
    pub strip: crate::git::StripState,
    pub compared_stripped: bool,
}

pub(super) fn is_notebook(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case("ipynb"))
}

/// The diff of the notebook `path` for `key`, when both sides that are there read as
/// notebooks; else `None`, and it shows as JSON.
pub(super) fn load(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    renamed_from: Option<&str>,
) -> Option<NotebookData> {
    let pair = super::images::load_images(repo, key, path, renamed_from);
    let read = |b: &Option<std::sync::Arc<[u8]>>| match b {
        Some(b) => parse::parse(b).map(Some),
        None => Ok(None),
    };
    let (old, new) = (read(&pair.old).ok()?, read(&pair.new).ok()?);
    if old.is_none() && new.is_none() {
        return None;
    }
    let strip = crate::git::strip_state(repo);
    // The working tree against the index: its outputs won't be staged, so they aren't
    // compared (git's own diff, through the filter, shows them out too).
    let unstaged = matches!(key, DiffKey::Work(f) if f.area == crate::git::Area::Unstaged);
    let compared_stripped = unstaged && strip != crate::git::StripState::Off;
    let new = if compared_stripped {
        new.map(parse::Notebook::stripped)
    } else {
        new
    };
    Some(NotebookData {
        diff: compare::compare(old.as_ref(), new.as_ref()),
        strip,
        compared_stripped,
    })
}

#[cfg(test)]
mod tests;
