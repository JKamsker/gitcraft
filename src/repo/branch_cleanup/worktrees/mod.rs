//! Cleaning up the worktrees of merged branches (#213), offered beside _Select stale or merged
//! branches…_: the worktrees whose branch the branch read found merged, each looked at (what
//! it holds, what keeps it), removed only when ticked, confirmed and still as previewed; their
//! untracked text files archived first when asked, and archives restored by hand or, when set,
//! when the branch is checked out again. The engine is `git::worktree_cleanup`.

mod archives;
mod jobs;
mod rows;
mod window;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use crate::git::worktree_cleanup::archive::Listed;
use crate::git::worktree_cleanup::{Evidence, Inspection, Outcome, Proof, Target, inspect};
use crate::repo::RepoTab;
use crate::repo::branch_health::{Bases, Merged, Reason};
use crate::repo::dialogs::CleanupAct;
use crate::settings::Settings;

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
            move || Ok((store.list_for(&repo)?, store.usage())),
            |tab, listed: Result<(Vec<Listed>, u64), String>, _| {
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.archives = Some(listed);
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
