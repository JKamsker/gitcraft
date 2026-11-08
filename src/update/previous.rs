//! The version an update replaced, kept beside this one (`gitcraft.previous`, its version in
//! `gitcraft.previous-version`) so that it can be gone back to: _Go back to 0.3.3_ swaps the
//! two, and the version gone back to is pinned (no update is offered) until it is unpinned.
//!
//! Every step is a rename inside the program's own folder, which Windows allows of a running
//! program, and each one is undone if a later one fails: the program's name always holds a
//! whole version.

use std::fs;
use std::path::{Path, PathBuf};

use super::witness::{Failure, PlacementFailure};

/// The version kept to go back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Previous {
    /// Its version, when it was written down.
    pub version: Option<String>,
    pub(super) path: PathBuf,
}

impl Previous {
    /// Newer than this build: going to it is going forward to the version gone back from.
    pub fn is_newer(&self) -> bool {
        self.version
            .as_deref()
            .is_some_and(|v| super::newer(v, super::CURRENT))
    }

    /// "0.3.3", or a plain name when its version wasn't written down.
    pub fn label(&self) -> &str {
        self.version.as_deref().unwrap_or("the version kept")
    }
}

/// `target` with `suffix` added to its file name.
fn beside(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_owned();
    name.push(suffix);
    target.with_file_name(name)
}

/// Where the previous version of `target` is kept.
pub(super) fn kept_path(target: &Path) -> PathBuf {
    beside(target, ".previous")
}

/// The file holding the kept version's version.
fn version_file(target: &Path) -> PathBuf {
    beside(target, ".previous-version")
}

/// Where `target` moves while another version takes its name.
pub(super) fn aside(target: &Path) -> PathBuf {
    beside(target, ".old")
}

/// The version kept beside `target`, if a whole one is.
pub(super) fn find(target: &Path) -> Option<Previous> {
    let path = kept_path(target);
    if !is_whole(&path) {
        return None;
    }
    let version = fs::read_to_string(version_file(target))
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| plain_version(v));
    Some(Previous { version, path })
}

/// A program file that isn't empty, or an app bundle with its program in it.
fn is_whole(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => path.join("Contents/MacOS/gitcraft").is_file(),
        Ok(m) => m.is_file() && m.len() > 0,
        Err(_) => false,
    }
}

/// A version as releases are named, nothing that could reach into a path or a label.
fn plain_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.starts_with(|c: char| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

/// Puts `new` in place of `target` (the running version, `current`), which is kept as the
/// previous version. `move_into` moves a file or bundle (a copy across drives).
pub(super) fn replace_keeping(
    target: &Path,
    new: &Path,
    current: &str,
    move_into: impl Fn(&Path, &Path) -> Result<(), String>,
) -> Result<(), String> {
    replace_keeping_witnessed(target, new, current, |from, to| {
        move_into(from, to).map_err(PlacementFailure::Opaque)
    })
    .map_err(Failure::into_raw)
}

pub(super) fn replace_keeping_witnessed(
    target: &Path,
    new: &Path,
    current: &str,
    move_into: impl Fn(&Path, &Path) -> Result<(), PlacementFailure>,
) -> Result<(), Failure> {
    let old = aside(target);
    remove(&old);
    fs::rename(target, &old).map_err(|e| move_aside_failure(target, e.to_string()))?;
    if let Err(e) = move_into(new, target) {
        let _ = fs::rename(&old, target);
        return Err(placement_failure(e));
    }
    keep(target, &old, current);
    Ok(())
}

/// Swaps `target` (the running version, `current`) with the version kept beside it, which
/// then keeps `current` to go forward to again. Returns the version gone back to.
pub(super) fn swap_back(target: &Path, current: &str) -> Result<Previous, String> {
    swap_back_witnessed(target, current).map_err(Failure::into_raw)
}

pub(super) fn swap_back_witnessed(target: &Path, current: &str) -> Result<Previous, Failure> {
    let previous = find(target).ok_or_else(no_previous)?;
    let old = aside(target);
    remove(&old);
    fs::rename(target, &old).map_err(|e| move_aside_failure(target, e.to_string()))?;
    if let Err(e) = fs::rename(&previous.path, target) {
        if fs::rename(&old, target).is_err() {
            return Err(restore_failure(
                &previous,
                e.to_string(),
                &old,
                target,
                true,
            ));
        }
        return Err(restore_failure(
            &previous,
            e.to_string(),
            &old,
            target,
            false,
        ));
    }
    keep(target, &old, current);
    Ok(previous)
}

/// Keeps `old` (version `version`) as the previous version of `target`, in place of the one
/// kept so far. If it can't be, it is left as `.old`, removed on the next start.
fn keep(target: &Path, old: &Path, version: &str) {
    let kept = kept_path(target);
    let versions = version_file(target);
    // Never a version file naming what was kept before.
    let _ = fs::remove_file(&versions);
    remove(&kept);
    // Still there (locked, on Windows): nothing is kept rather than the wrong one.
    if kept.exists() || fs::rename(old, &kept).is_err() {
        return;
    }
    if plain_version(version) && fs::write(&versions, version).is_err() {
        let _ = fs::remove_file(&versions);
    }
}

pub(super) fn remove(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// The pin to keep on a start of version `current`, from the one saved. Going back saves the
/// version gone back to, or [`PIN_THIS`] when it wasn't known; a pin of another version (the
/// user installed one by hand since) no longer holds.
pub fn settle_pin(saved: &str, current: &str) -> String {
    match saved {
        "" => String::new(),
        PIN_THIS => current.to_owned(),
        v if v == current => v.to_owned(),
        _ => String::new(),
    }
}

/// Saved as the pin when the version gone back to wasn't known: it is the one that starts.
pub const PIN_THIS: &str = "this version";

#[cfg(test)]
mod tests;

fn no_previous() -> Failure {
    Failure::NoPrevious {
        raw: "No previous version of gitcraft is kept".to_owned(),
    }
}
fn move_aside_failure(target: &Path, e: String) -> Failure {
    Failure::MoveAside {
        raw: format!("Could not move {} aside: {e}", target.display()),
        target: target.display().to_string(),
        detail: e,
    }
}
pub(super) fn placement_failure(failure: PlacementFailure) -> Failure {
    Failure::PutNew {
        raw: format!("Could not put the new version in place: {}", failure.raw()),
        failure,
    }
}
fn restore_failure(
    previous: &Previous,
    detail: String,
    old: &Path,
    target: &Path,
    rollback_failed: bool,
) -> Failure {
    if rollback_failed {
        return Failure::RestoreBoth {
            raw: format!(
                "Could not put gitcraft {} back in place ({detail}), nor this version: it is at {}; rename it back to {} by hand",
                previous.label(),
                old.display(),
                target.display()
            ),
            previous: previous.version.clone(),
            detail,
            old: old.display().to_string(),
            target: target.display().to_string(),
        };
    }
    Failure::RestorePrevious {
        raw: format!(
            "Could not put gitcraft {} back in place: {detail}",
            previous.label()
        ),
        previous: previous.version.clone(),
        detail,
    }
}

#[cfg(test)]
#[path = "previous/witness_tests.rs"]
mod witness_tests;
