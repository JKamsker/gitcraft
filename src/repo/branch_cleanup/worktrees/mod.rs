//! Cleaning up the worktrees of merged branches (#213), offered beside _Select stale or merged
//! branches…_: the worktrees whose branch the branch read found merged, each looked at (what
//! it holds, what keeps it), removed only when ticked, confirmed and still as previewed; their
//! untracked text files archived first when asked, and archives restored by hand or, when set,
//! when the branch is checked out again. The engine is `git::worktree_cleanup`.

mod archives;
mod rows;
mod window;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::git::worktree_cleanup::archive::{self, Listed, RestoreReport, Store};
use crate::git::worktree_cleanup::{Evidence, Inspection, Outcome, Proof, Target, inspect};
use crate::repo::branch_health::{Bases, Merged, Reason};
use crate::repo::{Notice, RepoTab};
use crate::settings::Settings;

/// What _Select stale or merged branches…_ opens: the branches, or the worktrees.
pub(crate) enum CleanupAct {
    Branches,
    Worktrees,
}

/// The tab's worktree cleanup: its window, and the automatic restores.
#[derive(Default)]
pub(in crate::repo) struct WorktreeCleanup {
    window: Option<Window>,
    /// The worktrees (folder, branch, commit) automatic restores last looked at.
    auto_seen: Option<Vec<(PathBuf, String, String)>>,
    auto_running: bool,
}

/// A worktree offered, as last looked at, and whether it is ticked.
struct Row {
    target: Target,
    found: Option<Result<Inspection, String>>,
    ticked: bool,
    open: bool,
}

/// The window: the worktrees, the choices, what was done, and this repository's archives.
struct Window {
    rows: Vec<Row>,
    /// Which branch merged means merged into, as the preview says.
    basis: String,
    /// Archive untracked text files first (opt-in).
    archive: bool,
    /// The explicit confirmation.
    reviewed: bool,
    reading: u64,
    running: bool,
    results: Vec<(String, Outcome)>,
    /// This repository's archives, and what every archive takes together.
    archives: Option<Result<(Vec<Listed>, u64), String>>,
    /// The worktree each archive would be restored into, by archive id.
    restore_into: std::collections::HashMap<String, PathBuf>,
    /// What the last restore or delete did.
    said: Option<(String, bool)>,
    /// The archive Delete was clicked for once.
    armed: Option<String>,
}

impl RepoTab {
    pub(in crate::repo) fn cleanup_act(&mut self, act: CleanupAct, ctx: &egui::Context) {
        match act {
            CleanupAct::Branches => self.dialog = Some(self.cleanup_dialog(ctx)),
            CleanupAct::Worktrees => self.open_worktree_cleanup(ctx),
        }
    }

    /// The worktrees whose branch is known merged, at the commit it was found merged at.
    pub(in crate::repo) fn merged_worktrees(&self) -> Vec<Target> {
        let Some(health) = &self.branch_health.health else {
            return Vec::new();
        };
        let bases = self.health_bases();
        self.worktrees
            .iter()
            .filter(|w| !w.main && !w.bare)
            .filter_map(|w| {
                let branch = w.branch.as_ref()?;
                let local = self.refs.local.iter().find(|b| &b.name == branch)?;
                let (tip, how) = health
                    .merged
                    .get(branch)
                    .filter(|(at, _)| *at == local.target)?;
                Some(Target {
                    path: w.path.clone(),
                    branch: branch.clone(),
                    evidence: evidence(how, tip, &bases)?,
                    in_use: None,
                })
            })
            .collect()
    }

    fn open_worktree_cleanup(&mut self, ctx: &egui::Context) {
        if self
            .worktree_cleanup
            .window
            .as_ref()
            .is_some_and(|w| w.running)
        {
            return;
        }
        let bases = self.health_bases();
        let refs: Vec<String> = bases.refs.iter().map(|(full, _)| short(full)).collect();
        let basis = crate::i18n::message(
            "worktree-cleanup-basis",
            &[
                ("base", bases.name.clone().into()),
                ("refs", refs.join(", ").into()),
            ],
        );
        self.worktree_cleanup.window = Some(Window {
            rows: Vec::new(),
            basis,
            archive: false,
            reviewed: false,
            reading: 0,
            running: false,
            results: Vec::new(),
            archives: None,
            restore_into: Default::default(),
            said: None,
            armed: None,
        });
        self.read_worktrees(ctx);
        self.read_archives(ctx);
    }

    /// Looks at every offered worktree again, in the background; a newer look replaces it.
    fn read_worktrees(&mut self, ctx: &egui::Context) {
        let targets = self.merged_worktrees();
        let Some(window) = &mut self.worktree_cleanup.window else {
            return;
        };
        window.reading += 1;
        let reading = window.reading;
        let kept: Vec<(PathBuf, bool, bool)> = window
            .rows
            .iter()
            .map(|r| (r.target.path.clone(), r.ticked, r.open))
            .collect();
        window.rows = targets
            .iter()
            .map(|target| {
                let was = kept.iter().find(|(p, ..)| *p == target.path);
                Row {
                    target: target.clone(),
                    found: None,
                    ticked: was.is_some_and(|w| w.1),
                    open: was.is_some_and(|w| w.2),
                }
            })
            .collect();
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                targets
                    .iter()
                    .map(|t| (t.path.clone(), inspect(&repo, t)))
                    .collect::<Vec<_>>()
            },
            move |tab, found, _| {
                let Some(window) = &mut tab.worktree_cleanup.window else {
                    return;
                };
                if window.reading != reading {
                    return;
                }
                for (path, result) in found {
                    if let Some(row) = window.rows.iter_mut().find(|r| r.target.path == path) {
                        row.found = Some(result);
                    }
                }
                window.reading = 0;
                window.untick_blocked();
            },
        );
    }

    /// This repository's archives, read in the background.
    fn read_archives(&mut self, ctx: &egui::Context) {
        let Some(store) = Settings::get(ctx).worktree_archives.store() else {
            return;
        };
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let key = archive::repository(&repo)?.key;
                Ok((store.list(Some(&key)), store.usage()))
            },
            |tab, listed: Result<(Vec<Listed>, u64), String>, _| {
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.archives = Some(listed);
                }
            },
        );
    }

    /// Removes the ticked worktrees, one after the other, each looked at again first.
    fn run_cleanup(&mut self, ctx: &egui::Context) {
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
    fn restore_archive(&mut self, listed: Listed, into: PathBuf, ctx: &egui::Context) {
        let Some(store) = Settings::get(ctx).worktree_archives.store() else {
            return;
        };
        self.then(
            ctx,
            move || archive::restore(&store, &listed, &into).map(|r| (into, r)),
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

impl Window {
    /// The ticked worktrees that nothing keeps, with what they were previewed as.
    fn chosen(&self) -> Vec<(Target, Inspection)> {
        self.rows
            .iter()
            .filter(|r| r.ticked)
            .filter_map(|r| match &r.found {
                Some(Ok(found)) if found.blockers_with(self.archive).is_empty() => {
                    Some((r.target.clone(), found.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// Unticks what something keeps now (archiving turned off, a new look).
    fn untick_blocked(&mut self) {
        let archive = self.archive;
        for row in &mut self.rows {
            let free = matches!(&row.found, Some(Ok(f)) if f.blockers_with(archive).is_empty());
            row.ticked &= free;
        }
    }
}

/// The evidence the branch read found, with the full ref it was checked against.
pub(super) fn evidence(how: &Merged, tip: &str, bases: &Bases) -> Option<Evidence> {
    let (base_ref, proof) = match how {
        Merged::Into(base) => bases
            .refs
            .iter()
            .find(|(full, _)| short(full) == *base)
            .map(|(full, _)| (full.clone(), Proof::Ancestor))?,
        Merged::Rebased(_) => (bases.refs.first()?.0.clone(), Proof::PatchEquivalent),
        Merged::Pr(n, base) => (base.clone(), Proof::PullRequest(*n)),
    };
    Some(Evidence {
        base_ref,
        proof,
        tip: tip.to_owned(),
        describe: Reason::Merged(how.clone()).describe(),
    })
}

fn short(full: &str) -> String {
    full.strip_prefix("refs/heads/")
        .or_else(|| full.strip_prefix("refs/remotes/"))
        .unwrap_or(full)
        .to_owned()
}

/// Restores every archive of this repository not restored yet whose branch is checked out in
/// one of `worktrees` (folder, branch, commit) at the commit it was archived from: a branch
/// of the same name made again elsewhere gets nothing.
pub(super) fn auto_restore(
    repo: &Path,
    store: &Store,
    worktrees: &[(PathBuf, String, String)],
) -> Vec<(PathBuf, RestoreReport)> {
    let Ok(repository) = archive::repository(repo) else {
        return Vec::new();
    };
    let mut done = Vec::new();
    for listed in store.list(Some(&repository.key)) {
        let m = &listed.manifest;
        let matching = worktrees
            .iter()
            .find(|(_, branch, head)| *branch == m.branch && *head == m.head);
        if let (true, Some((path, ..))) = (m.restored.is_empty(), matching)
            && let Ok(report) = archive::restore(store, &listed, path)
        {
            done.push((path.clone(), report));
        }
    }
    done
}

/// "Restored 3 files into …; 1 beside a file already there (name.restored)".
fn describe_restore(into: &Path, report: &RestoreReport) -> String {
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
