//! Removing a worktree that was previewed: read again, compared with the preview, its
//! untracked text files archived and verified first when asked, then `git worktree remove`
//! without `--force`, so that git refuses whatever changed after this last look. When git
//! refuses, the archived files are put back.

use std::path::Path;

use super::archive::{self, Listed, Source, Store};
use super::files::classify;
use super::{Blocker, Footprint, Inspection, Kind, Target, inspect};

/// What became of one worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Removed, its untracked files in `archive` if it had any.
    Removed {
        archive: Option<Box<Listed>>,
        freed: Footprint,
    },
    /// Kept: what keeps it now.
    Kept(Vec<Blocker>),
    /// Kept: it changed since the preview, which must be looked at again.
    Changed,
    /// Kept: something failed; nothing in it was lost.
    Failed(String),
}

/// Removes `target` if it is still as `preview` showed it and nothing keeps it; its untracked
/// text files go to `store` first when `archive` (otherwise it must have none). Never when the
/// archives would go with it: `store` lies inside it, in an ignored folder too.
pub(crate) fn remove(
    repo: &Path,
    target: &Target,
    preview: &Inspection,
    store: Option<&Store>,
    archive: bool,
) -> Outcome {
    if store.is_some_and(|s| s.is_inside(&target.path)) {
        return Outcome::Failed(crate::i18n::text("worktree-cleanup-archive-inside").to_owned());
    }
    let store = store.filter(|_| archive);
    let now = match inspect(repo, target) {
        Ok(now) => now,
        Err(e) => return Outcome::Failed(e),
    };
    if now.fingerprint != preview.fingerprint || now.fingerprint.is_empty() {
        return if now.blockers.is_empty() {
            Outcome::Changed
        } else {
            Outcome::Kept(now.blockers)
        };
    }
    let blockers = now.blockers_with(store.is_some());
    if !blockers.is_empty() {
        return Outcome::Kept(blockers);
    }
    let Some(folder) = target.path.to_str() else {
        return Outcome::Failed(crate::i18n::text("worktree-cleanup-path-not-utf8").to_owned());
    };
    let archived = match (store, now.untracked.is_empty()) {
        (_, true) => None,
        (Some(store), false) => match archive_and_clear(store, target, &now) {
            Ok(listed) => Some((store, listed)),
            Err(e) => return Outcome::Failed(e),
        },
        (None, false) => return Outcome::Kept(vec![Blocker::Untracked(now.untracked.len())]),
    };
    match crate::git::run(Some(repo), &["worktree", "remove", "--", folder]) {
        Ok(_) => Outcome::Removed {
            archive: archived.map(|(_, listed)| Box::new(listed)),
            freed: now.footprint,
        },
        Err(e) => Outcome::Failed(match archived {
            Some((store, listed)) => put_back(store, &listed, &target.path, &e),
            None => e,
        }),
    }
}

/// Archives the untracked files, then takes each out of the worktree, as checked just before;
/// should one fail, those taken out are put back and the archive goes.
fn archive_and_clear(store: &Store, target: &Target, now: &Inspection) -> Result<Listed, String> {
    let repository = archive::repository(&target.path)?;
    let listed = archive::create(
        store,
        &Source {
            repository: &repository,
            worktree: &target.path,
            branch: &target.branch,
            head: &now.head,
            files: &now.untracked,
        },
    )
    .map_err(|e| crate::i18n::message("worktree-cleanup-archive-failed", &[("error", e.into())]))?;
    for file in &now.untracked {
        let still = matches!(
            (&file.kind, classify(&target.path, &file.path).1),
            (Kind::Text(a), Kind::Text(b)) if *a == b
        );
        let removed = still && std::fs::remove_file(target.path.join(&file.path)).is_ok();
        if !removed {
            let e = crate::i18n::message(
                "worktree-cleanup-archive-changed",
                &[("path", file.path.clone().into())],
            );
            return Err(put_back(store, &listed, &target.path, &e));
        }
    }
    Ok(listed)
}

/// After `error`, puts the archived files back where they were; the archive goes once all are
/// back. Says what happened.
fn put_back(store: &Store, listed: &Listed, worktree: &Path, error: &str) -> String {
    match store.restore(listed, worktree) {
        Ok(report) if report.failed.is_empty() && report.renamed.is_empty() => {
            // Every file is back where it was: the archive would only be a second copy.
            let _ = store.delete(listed);
            crate::i18n::message(
                "worktree-cleanup-put-back",
                &[("error", error.to_owned().into())],
            )
        }
        _ => crate::i18n::message(
            "worktree-cleanup-put-back-failed",
            &[
                ("error", error.to_owned().into()),
                ("archive", listed.zip.to_string_lossy().into_owned().into()),
            ],
        ),
    }
}
