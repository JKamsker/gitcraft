//! External diff and merge tools (Beyond Compare, Meld, VS Code…): a file's diff opened in one,
//! a conflict resolved in one. They start through `git difftool` / `git mergetool` on the
//! repository's machine, with the tool git's config names or a preset chosen in Settings, so
//! git's own settings for them (`mergetool.<tool>.trustExitCode`, `mergetool.keepBackup`,
//! `.path`, `.cmd`) apply as they do in a terminal.
//!
//! In WSL, a preset is the Windows program of this machine, handed the files as
//! `\\wsl.localhost` paths; over SSH, the tool runs on that machine and has to be installed
//! there.

mod plan;
mod preset;
mod run;
mod target;

#[cfg(test)]
mod tests;

use std::path::Path;

pub use plan::Place;
pub use preset::{Kind, Preset, ToolChoice};
pub use run::{MergeOutcome, open_diff, place_of, resolve_conflict};
pub use target::DiffTarget;

/// What a commit changed in `path`: from its first parent (nothing, for a first commit) to it,
/// from where it was renamed from if it was.
pub fn commit_target(repo: &Path, hash: &str, path: &str) -> Result<DiffTarget, String> {
    let from = run::parent_of(repo, hash)?;
    let old_path = run::renamed_from(repo, &from, Some(hash), path);
    Ok(DiffTarget::Revisions {
        from,
        to: Some(hash.to_owned()),
        path: path.to_owned(),
        old_path,
    })
}

/// `path` from `from` to `to` (a commit, or the working tree when `None`), from where it was
/// renamed from if it was.
pub fn range_target(repo: &Path, from: &str, to: Option<&str>, path: &str) -> DiffTarget {
    DiffTarget::Revisions {
        from: from.to_owned(),
        to: to.map(str::to_owned),
        path: path.to_owned(),
        old_path: run::renamed_from(repo, from, to, path),
    }
}
