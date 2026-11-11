//! Changed-file selection and stash eligibility checks.

use super::*;

pub(super) fn all_files(work: &WorkStatus) -> impl Iterator<Item = &git::StatusFile> {
    work.conflicted
        .iter()
        .chain(&work.staged)
        .chain(&work.unstaged)
}

/// Why the file of the row `f` can't be stashed by its name, if it can't: `git stash push --
/// <paths>` refuses a path only HEAD knows (a staged deletion, the old side of a staged rename),
/// and with untracked files it does so after saving the stash, the changes left in place.
/// Messages read after "<operation> failed: ", so they start in lower case.
pub(crate) fn row_problem(f: &git::StatusFile) -> Option<String> {
    let path = &f.path;
    if f.area == git::Area::Conflicted {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-a2071523",
            &[("path", (path).clone().into())],
        ));
    }
    if let Some(old) = &f.old_path {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-78f72f91",
            &[
                ("old", (old).clone().into()),
                ("path", (path).clone().into()),
            ],
        ));
    }
    if f.area == git::Area::Staged && f.code == 'D' {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-140ed58d",
            &[("path", (path).clone().into())],
        ));
    }
    None
}

/// How long the picked paths may be together: they go on one command line, which Windows
/// keeps under 32 767 characters.
pub(super) const PATHS_BUDGET: usize = 24 * 1024;

/// Why git can't stash just `paths` as they are in `work`, if it can't (see [`row_problem`]).
pub(crate) fn files_problem(paths: &[String], work: &WorkStatus) -> Option<String> {
    if paths.is_empty() {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-files-problem-bdb546dd",
            &[("crate-manual-command-1", (crate::manual::command()).into())],
        ));
    }
    let bytes: usize = paths.iter().map(|p| p.len() + 12).sum();
    if bytes > PATHS_BUDGET {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-files-problem-0a39f709",
            &[("paths-len-1", (paths.len()).into())],
        ));
    }
    for path in paths {
        let mut entries = all_files(work).filter(|f| &f.path == path).peekable();
        if entries.peek().is_none() {
            return Some(crate::i18n::message(
                "repo-workflows-partial_stash-files-problem-9175a257",
                &[("path", (path).clone().into())],
            ));
        }
        if let Some(problem) = entries.find_map(row_problem) {
            return Some(problem);
        }
    }
    outside_staged_problem(paths, work)
}

/// Git saves the whole index even with pathspecs. Leaving staged paths outside the
/// selection makes the generic `stash pop --index` Undo try to apply them twice.
pub(super) fn outside_staged_problem(paths: &[String], work: &WorkStatus) -> Option<String> {
    work.staged
        .iter()
        .find(|f| !paths.contains(&f.path))
        .map(|f| {
            crate::i18n::message(
                "repo-workflows-partial_stash-outside-staged",
                &[("path", f.path.clone().into())],
            )
        })
}

/// Whether `work` has changes besides untracked files (what a stash without them can take).
pub(crate) fn has_tracked_changes(work: &WorkStatus) -> bool {
    !work.staged.is_empty()
        || !work.conflicted.is_empty()
        || work.unstaged.iter().any(|f| f.area != git::Area::Untracked)
}

/// Why stashing everything (or a snapshot) can't be done now, if it can't.
pub(crate) fn everything_problem(work: &WorkStatus, untracked: bool) -> Option<String> {
    if work.is_clean() {
        Some(crate::i18n::text("repo-workflows-partial_stash-everything-problem-a7f01d16").into())
    } else if !untracked && !has_tracked_changes(work) {
        Some(crate::i18n::text("repo-workflows-partial_stash-everything-problem-63cc5481").into())
    } else {
        None
    }
}
