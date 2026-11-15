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
        let archive = window.archive;
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
                for (mut target, preview) in chosen {
                    target.in_use = match current_use(&repo, &target.path) {
                        Ok(reason) => reason,
                        Err(e) => {
                            done.push((target.path.display().to_string(), Outcome::Failed(e)));
                            continue;
                        }
                    };
                    let name = target.path.to_string_lossy().into_owned();
                    done.push((
                        name,
                        crate::git::worktree_cleanup::remove(
                            &repo,
                            &target,
                            &preview,
                            store.as_ref(),
                            archive,
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
        let worktrees = restore_targets(&self.worktrees);
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

/// The worktrees (folder, branch, commit) an archive may come back into by itself: never the
/// main one (archives come from worktrees that were removed), a bare one or one gone.
pub(super) fn restore_targets(
    worktrees: &[crate::git::Worktree],
) -> Vec<(PathBuf, String, String)> {
    worktrees
        .iter()
        .filter(|w| !w.main && !w.bare && w.prunable.is_none())
        .filter_map(|w| Some((w.path.clone(), w.branch.clone()?, w.head.clone())))
        .collect()
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

/// Recheck app presence and persisted tasks immediately before each removal.
pub(super) fn current_use(repo: &Path, path: &Path) -> Result<Option<String>, String> {
    let mut reasons = super::presence::Presence::using(path, None);
    let same = crate::remote::is_same_dir_as(path);
    let root = path.canonicalize().unwrap_or_else(|_| path.to_owned());
    for task in crate::agent::tasks::load(repo)?.iter().filter(|t| {
        same(Path::new(&t.path))
            || Path::new(&t.path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(&t.path))
                .starts_with(&root)
    }) {
        reasons.push(crate::i18n::message(
            "worktree-cleanup-in-use-agent",
            &[(
                "agent",
                crate::agent::tasks::agent_label(&task.agent).into(),
            )],
        ));
    }
    Ok((!reasons.is_empty()).then(|| reasons.join("; ")))
}
