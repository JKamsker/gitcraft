//! Recoverable cleanup: move the complete tree first, verify there, optionally archive,
//! then retain its private Git metadata. User content is never unlinked by cleanup.
use std::path::{Path, PathBuf};

use super::archive::{self, Listed, Source, Store};
use super::recovery::Capture;
use super::{Blocker, Footprint, Inspection, Target, inspect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Removed {
        archive: Option<Box<Listed>>,
        freed: Footprint,
        recovery: PathBuf,
    },
    Kept(Vec<Blocker>),
    Changed,
    Failed(String),
}

pub(crate) fn remove(
    repo: &Path,
    target: &Target,
    preview: &Inspection,
    store: Option<&Store>,
    archive: bool,
) -> Outcome {
    remove_after_inspection(repo, target, preview, store, archive, || {})
}

/// The hook makes a writer between final inspection and capture deterministic in tests.
fn remove_after_inspection(
    repo: &Path,
    target: &Target,
    preview: &Inspection,
    store: Option<&Store>,
    archive: bool,
    after: impl FnOnce(),
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
    after();
    let captured = match Capture::move_tree(repo, target) {
        Ok(captured) => captured,
        Err(e) => return Outcome::Failed(e),
    };
    let result = finish(repo, target, &now, store, &captured);
    match result {
        Ok(archive) => Outcome::Removed {
            archive: archive.map(Box::new),
            // Retained copies consume disk until explicitly purged.
            freed: Footprint {
                complete: true,
                ..Footprint::default()
            },
            recovery: captured.tree(),
        },
        Err(e) => Outcome::Failed(captured.rollback(&e)),
    }
}

fn finish(
    repo: &Path,
    target: &Target,
    preview: &Inspection,
    store: Option<&Store>,
    captured: &Capture,
) -> Result<Option<Listed>, String> {
    let tree = captured.tree();
    let registered = Target {
        path: captured.registered.clone(),
        ..target.clone()
    };
    let now = super::inspect::at(repo, &registered, &tree, true)?;
    if now.fingerprint != preview.fingerprint
        || now.fingerprint.is_empty()
        || !now.blockers_with(store.is_some()).is_empty()
    {
        return Err(crate::i18n::text("worktree-cleanup-changed").to_owned());
    }
    let archived = if !now.untracked.is_empty() {
        let store = store.ok_or("Archiving was not selected")?;
        let repository = archive::repository(repo)?;
        Some(archive::create(
            store,
            &Source {
                repository: &repository,
                worktree: &target.path,
                read_root: &tree,
                branch: &target.branch,
                head: &now.head,
                files: &now.untracked,
            },
        )?)
    } else {
        None
    };
    // No classify-then-unlink. Even writes through descriptors opened before rename remain
    // in the retained tree, whether they precede or follow archive verification.
    captured.unregister()?;
    Ok(archived)
}

#[cfg(test)]
#[path = "tests/transaction_tests.rs"]
mod tests;
