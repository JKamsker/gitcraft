//! The version an update replaced, kept beside this one (`gitcraft.previous`, its version in
//! `gitcraft.previous-version`) so that it can be gone back to: _Go back to 0.3.3_ swaps the
//! two, and the version gone back to is pinned (no update is offered) until it is unpinned.
//!
//! Every step is a rename inside the program's own folder, which Windows allows of a running
//! program, and each one is undone if a later one fails: the program's name always holds a
//! whole version.

use std::fs;
use std::path::{Path, PathBuf};

/// The version kept to go back to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Previous {
    /// Its version, when it was written down.
    pub version: Option<String>,
    pub(super) path: PathBuf,
}

impl Previous {
    /// "0.3.3", or a plain name when its version wasn't written down.
    pub fn label(&self) -> &str {
        self.version.as_deref().unwrap_or("the previous version")
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
    let old = aside(target);
    remove(&old);
    fs::rename(target, &old)
        .map_err(|e| format!("Could not move {} aside: {e}", target.display()))?;
    if let Err(e) = move_into(new, target) {
        let _ = fs::rename(&old, target);
        return Err(format!("Could not put the new version in place: {e}"));
    }
    keep(target, &old, current);
    Ok(())
}

/// Swaps `target` (the running version, `current`) with the version kept beside it, which
/// then keeps `current` to go forward to again. Returns the version gone back to.
pub(super) fn swap_back(target: &Path, current: &str) -> Result<Previous, String> {
    let previous = find(target).ok_or("No previous version of gitcraft is kept")?;
    let old = aside(target);
    remove(&old);
    fs::rename(target, &old)
        .map_err(|e| format!("Could not move {} aside: {e}", target.display()))?;
    if let Err(e) = fs::rename(&previous.path, target) {
        let _ = fs::rename(&old, target);
        return Err(format!(
            "Could not put gitcraft {} back in place: {e}",
            previous.label()
        ));
    }
    keep(target, &old, current);
    Ok(previous)
}

/// Keeps `old` (version `version`) as the previous version of `target`, in place of the one
/// kept so far. If it can't be, it is left as `.old`, removed on the next start.
fn keep(target: &Path, old: &Path, version: &str) {
    let kept = kept_path(target);
    remove(&kept);
    let versions = version_file(target);
    if fs::rename(old, &kept).is_ok() && plain_version(version) {
        let _ = fs::write(&versions, version);
    } else {
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
