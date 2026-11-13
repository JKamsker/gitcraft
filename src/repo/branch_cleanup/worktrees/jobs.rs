//! What the window and the reloads start in the background: the cleanup itself, restoring an
//! archive by hand, and restoring archives by themselves when the settings ask for it.

use std::path::{Path, PathBuf};

use crate::git::worktree_cleanup::Outcome;
use crate::git::worktree_cleanup::archive::{Listed, RestoreReport, Store};
use crate::repo::{Notice, RepoTab};
use crate::settings::Settings;

impl RepoTab {
    /// Removes the ticked worktrees, one after the other, each looked at again first.
    pub(super) fn run_cleanup(&mut self, ctx: &egui::Context) {
        let store = Settings::get(ctx).worktree_archives.store();
        let Some(window) = &mut self.worktree_cleanup.window else {
            return;
        };
        let chosen = window.chosen();
        if chosen.is_empty() || window.running || self.busy_op.is_some() {
            return;
        }
        let store = if window.archive { store } else { None };
        window.running = true;
        window.results.clear();
        let repo = self.path.clone();
        let failed = |e: String| vec![(String::new(), Outcome::Failed(e))];
        self.then_or(
            ctx,
            true,
            failed,
            move || {
                let mut done = Vec::new();
                for (target, preview) in chosen {
                    let name = target.path.to_string_lossy().into_owned();
                    done.push((
                        name,
                        crate::git::worktree_cleanup::remove(
                            &repo,
                            &target,
                            &preview,
                            store.as_ref(),
                        ),
                    ));
                }
                done
            },
            |tab, done, ctx| {
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.running = false;
                    window.reviewed = false;
                    window.results = done;
                }
                tab.reload(ctx);
                tab.read_worktrees(ctx);
                tab.read_archives(ctx);
            },
        );
    }

    /// Restores `listed` into the worktree `into`, then reads the archives again.
    pub(super) fn restore_archive(&mut self, listed: Listed, into: PathBuf, ctx: &egui::Context) {
        let Some(store) = Settings::get(ctx).worktree_archives.store() else {
            return;
        };
        self.then(
            ctx,
            move || store.restore(&listed, &into).map(|r| (into, r)),
            |tab, result, ctx| {
                let said = match result {
                    Ok((into, report)) => {
                        (describe_restore(&into, &report), !report.failed.is_empty())
                    }
                    Err(e) => (e, true),
                };
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.said = Some(said);
                }
                tab.read_archives(ctx);
                tab.reload_status(ctx);
            },
        );
    }

    /// Restores archives whose branch is checked out again in a worktree at the commit it was
    /// archived from, when the settings ask for it; each archive once.
    pub(in crate::repo) fn want_auto_restore(&mut self, ctx: &egui::Context) {
        let settings = Settings::get(ctx).worktree_archives;
        let busy = self
            .worktree_cleanup
            .window
            .as_ref()
            .is_some_and(|w| w.running);
        if !settings.auto_restore || busy || self.worktree_cleanup.auto_running {
            return;
        }
        let Some(store) = settings
            .store()
            .filter(|_| !crate::remote::is_remote(&self.path))
        else {
            return;
        };
        let worktrees: Vec<(PathBuf, String, String)> = self
            .worktrees
            .iter()
            .filter(|w| !w.bare && w.prunable.is_none())
            .filter_map(|w| Some((w.path.clone(), w.branch.clone()?, w.head.clone())))
            .collect();
        if self.worktree_cleanup.auto_seen.as_ref() == Some(&worktrees) {
            return;
        }
        self.worktree_cleanup.auto_seen = Some(worktrees.clone());
        self.worktree_cleanup.auto_running = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || auto_restore(&repo, &store, &worktrees),
            |tab, done, ctx| {
                tab.worktree_cleanup.auto_running = false;
                let said: Vec<String> = done
                    .iter()
                    .map(|(into, report)| describe_restore(into, report))
                    .collect();
                if !said.is_empty() {
                    tab.status = Some(Notice::plain((said.join("\n"), false)));
                    tab.reload_status(ctx);
                }
            },
        );
    }
}

/// Restores every archive of this repository not restored yet whose branch is checked out in
/// one of `worktrees` (folder, branch, commit) at the commit it was archived from: a branch
/// of the same name made again elsewhere gets nothing.
pub(super) fn auto_restore(
    repo: &Path,
    store: &Store,
    worktrees: &[(PathBuf, String, String)],
) -> Vec<(PathBuf, RestoreReport)> {
    let Ok(archives) = store.list_for(repo) else {
        return Vec::new();
    };
    let mut done = Vec::new();
    for listed in archives {
        let matching = worktrees
            .iter()
            .find(|(_, branch, head)| listed.awaits(branch, head));
        if let Some((path, ..)) = matching
            && let Ok(report) = store.restore(&listed, path)
        {
            done.push((path.clone(), report));
        }
    }
    done
}

/// "Restored 3 files into …; 1 beside a file already there (name.restored)".
pub(super) fn describe_restore(into: &Path, report: &RestoreReport) -> String {
    crate::i18n::message(
        "worktree-cleanup-restored",
        &[
            ("created", report.created.len().into()),
            ("renamed", report.renamed.len().into()),
            ("same", report.same.len().into()),
            ("failed", report.failed.len().into()),
            ("into", into.to_string_lossy().into_owned().into()),
        ],
    )
}
