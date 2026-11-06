//! What a merge resolved: the difference between the merge git would make of a merge commit's
//! parents today and the one that was recorded (`git show --remerge-diff`, git 2.36 and later).
//! What is left is how its conflicts were resolved and anything edited beyond the merge.

mod parse;
#[cfg(test)]
mod tests;

use std::path::Path;

pub use self::parse::{is_conflict_marker, parse_git_version, parse_remerge};
use super::{DiffOptions, END_OPTS, LITERAL_PATHS, NO_SIGNATURE, run};

/// The first git that can show a remerge diff.
pub const REMERGE_MIN_GIT: (u32, u32) = (2, 36);

/// One file of a remerge diff.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemergeFile {
    /// `M`, `A`, `D` or `R`, from the recorded merge's side.
    pub status: char,
    pub path: String,
    /// The path in git's own merge, for a file the recorded one has under another name.
    pub old_path: Option<String>,
    /// Lines the recorded merge has that git's own doesn't, and the other way round.
    pub added: u32,
    pub removed: u32,
    /// What git said of the conflicts it met in this file (`CONFLICT (content): …`), or of a
    /// conflict over several paths that names it.
    pub conflicts: Vec<String>,
    /// Hunks that resolve a conflict (see [`hunk_resolves`]).
    pub resolving_hunks: u32,
    /// Hunks that change what git merged without a conflict.
    pub edited_hunks: u32,
}

impl RemergeFile {
    /// git met a conflict in this file.
    pub fn conflicted(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// Something in it was changed beyond what the merge needed: it had no conflict, or one of
    /// its hunks is away from the conflicts.
    pub fn edited(&self) -> bool {
        !self.conflicted() || self.edited_hunks > 0
    }
}

/// Whether a hunk of a file of which git said `conflicts` resolves one of them rather than
/// editing beyond the merge: it takes away conflict markers (`marked`); or the conflict isn't
/// one of lines (modify/delete, rename, binary, submodule…), so any change resolves it; or no
/// hunk of the file has markers (`any_marked`: a link, a driver that writes none), so the
/// conflict was resolved somewhere among them.
pub fn hunk_resolves(conflicts: &[String], marked: bool, any_marked: bool) -> bool {
    !conflicts.is_empty()
        && (marked
            || !any_marked
            || conflicts
                .iter()
                .any(|c| !c.starts_with("CONFLICT (content)")))
}

/// The remerge diff of one merge commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remerge {
    pub files: Vec<RemergeFile>,
}

impl Remerge {
    /// The recorded merge is the one git makes: no conflicts and no other edits.
    pub fn is_clean(&self) -> bool {
        self.files.is_empty()
    }

    /// Files whose conflicts were resolved.
    pub fn conflicted(&self) -> impl Iterator<Item = &RemergeFile> {
        self.files.iter().filter(|f| f.conflicted())
    }

    /// Files with changes beyond what the merge needed (by hand, or a merge made with other
    /// options or another git than today's); a conflicted file can be one too.
    pub fn edited(&self) -> impl Iterator<Item = &RemergeFile> {
        self.files.iter().filter(|f| f.edited())
    }
}

/// `show` arguments for the remerge diff of `hash`, plain whatever the user's config says.
fn remerge_args() -> Vec<&'static str> {
    vec![
        LITERAL_PATHS,
        // Names outside ASCII as they are; git still quotes those with `"`, `\` or controls.
        "-c",
        "core.quotePath=false",
        "show",
        NO_SIGNATURE,
        "--format=",
        "--remerge-diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ]
}

/// Why a commit with `parents` parents has no remerge diff to show, if it hasn't: git redoes
/// only merges of two parents (it skips octopus merges, printing nothing).
pub fn remerge_unavailable(parents: usize) -> Option<String> {
    match parents {
        2 => None,
        0 | 1 => Some("Not a merge: there is nothing it resolved".to_owned()),
        n => Some(format!(
            "An octopus merge ({n} parents): git can only redo merges of two parents, so it \
             can't show what this one resolved"
        )),
    }
}

/// The remerge diff of the merge commit `hash` (of two parents: see [`remerge_unavailable`]),
/// read file by file.
pub fn remerge(repo: &Path, hash: &str) -> Result<Remerge, String> {
    let mut args = remerge_args();
    args.extend([END_OPTS, hash]);
    let out = run(Some(repo), &args).map_err(|e| explain(repo, &e))?;
    Ok(parse_remerge(&out))
}

/// The remerge diff of one file of the merge `hash` (`old_path`: its name in git's own merge,
/// when that is another).
pub fn remerge_file_diff(
    repo: &Path,
    hash: &str,
    path: &str,
    old_path: Option<&str>,
    opts: DiffOptions,
) -> Result<String, String> {
    let mut args = remerge_args();
    if opts.ignore_ws {
        args.push("-w");
    }
    if opts.whole_file {
        // More context than any file has lines: each file as one hunk.
        args.push("--unified=999999999");
    }
    args.extend([END_OPTS, hash, "--", path]);
    args.extend(old_path.filter(|old| *old != path));
    run(Some(repo), &args).map_err(|e| explain(repo, &e))
}

/// Whether the git that runs for `repo` (here, in WSL or over SSH) can show remerge diffs; if
/// not, why, to show the user.
pub fn remerge_support(repo: &Path) -> Result<(), String> {
    let version = run(Some(repo), &["--version"])?;
    supports_remerge(version.trim())
}

/// Whether `repo` has merge drivers of its own configured (`merge.<name>.driver`): git runs
/// them when it redoes a merge, so merges aren't redone unasked there.
pub fn has_merge_drivers(repo: &Path) -> bool {
    run(
        Some(repo),
        &["config", "--get-regexp", r"^merge\..*\.driver$"],
    )
    .is_ok_and(|out| !out.trim().is_empty())
}

/// Whether the git that printed `version` (`git --version`) can show remerge diffs.
pub fn supports_remerge(version: &str) -> Result<(), String> {
    let (major, minor) = REMERGE_MIN_GIT;
    match parse_git_version(version) {
        Some(found) if found >= REMERGE_MIN_GIT => Ok(()),
        Some((has_major, has_minor)) => Err(format!(
            "Needs git {major}.{minor} or newer; this repository's git is {has_major}.{has_minor}"
        )),
        None => Err(format!(
            "Needs git {major}.{minor} or newer; could not tell the version from “{version}”"
        )),
    }
}

/// An error of `git show --remerge-diff` as the user should read it.
pub(super) fn explain(repo: &Path, err: &str) -> String {
    if err.contains("remerge-diff") && (err.contains("unrecognized") || err.contains("unknown")) {
        return remerge_support(repo)
            .err()
            .unwrap_or_else(|| format!("This git can't show what a merge resolved: {err}"));
    }
    if err.contains("temporary object directory") {
        return "Could not redo the merge: git writes its objects to a temporary folder in the \
                repository's .git/objects to do so, and can't write there (it is read-only for \
                you)"
            .to_owned();
    }
    if ["bad object", "unable to read", "missing", "could not read"]
        .iter()
        .any(|s| err.contains(s))
    {
        return format!(
            "Could not redo the merge: a parent (or a file of one) isn't in this clone; \
             fetching more history may bring it ({err})"
        );
    }
    format!("Could not redo the merge to compare it: {err}")
}
