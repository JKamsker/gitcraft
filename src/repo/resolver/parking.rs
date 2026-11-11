//! A run on a merge put aside (see [`crate::resolver::park`]): the parked merges of the
//! worktree read with the rest of the tab, the agent's work kept with the merge when it is done
//! (its files and its run, in `.git/gitcraft/resolver/<id>/`, so that it outlasts gitcraft),
//! the user told, and the merge brought back to review and apply that work. A parked merge
//! whose run isn't in this tab (gitcraft was closed meanwhile) gets its run back from what was
//! kept, or waits to be brought back.

mod drop;
mod record;
pub(super) use record::preserve_uncertain;
use record::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{Phase, Run, done_files};
use crate::repo::RepoTab;
use crate::resolver::agent::Engine;
use crate::resolver::park::{self, Parked};
use crate::resolver::report::{PlannedStep, Progress, Report, StepStatus};
use crate::resolver::workspace::{FileResult, Workspace};

/// What the tab says it is doing while a merge is brought back, or dropped.
const BRINGING_BACK: &str = "Bring the merge back";
const DROPPING: &str = "Drop the merge put aside";

/// Drops `ws` away from the UI thread: dropping a scratch worktree runs git.
fn let_go(ws: Option<Arc<Workspace>>) {
    if let Some(ws) = ws {
        std::thread::spawn(move || {
            if let Ok(ws) = Arc::try_unwrap(ws) {
                let _ = ws.remove_patiently();
            }
        });
    }
}

impl RepoTab {
    /// Reads the merges put aside from this worktree (with the rest of the tab, on a reload).
    pub(in crate::repo) fn read_parked(&mut self, ctx: &egui::Context) {
        if crate::remote::is_remote(&self.path) {
            return;
        }
        // One at a time: the one under way is read again once it is in.
        if self.resolver.reading_parked {
            self.resolver.read_parked_again = true;
            return;
        }
        self.resolver.reading_parked = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || park::list(&repo),
            |tab, parked, ctx| tab.parked_read(parked, ctx),
        );
    }

    fn parked_read(&mut self, parked: Result<Vec<Parked>, String>, ctx: &egui::Context) {
        self.resolver.reading_parked = false;
        // Read before a change made here (or asked for since): read again, this one dropped.
        if std::mem::take(&mut self.resolver.read_parked_again) {
            self.read_parked(ctx);
            return;
        }
        let parked = match parked {
            Ok(parked) => parked,
            Err(error) => {
                self.status = Some(crate::repo::notice::Notice::plain((
                    format!("The recovery archives couldn't be read: {error}"),
                    true,
                )));
                ctx.request_repaint();
                return;
            }
        };
        if parked != self.resolver.parked {
            self.resolver.parked = parked;
            self.rebuild(ctx);
        }
        // One of them without its run here: gitcraft was closed meanwhile. Its run comes back
        // from what was kept (else it waits, in the graph, to be brought back).
        if self.resolver.run.is_some() {
            return;
        }
        let parked = self.resolver.parked.clone();
        if parked.is_empty() {
            return;
        }
        let repo = self.path.clone();
        self.then(
            ctx,
            // The first of them whose agent's work was kept.
            move || parked.iter().find_map(|p| kept_run(&repo, p)),
            |tab, run, _| tab.kept_run_read(run),
        );
    }

    /// The run of a parked merge, read back from what was kept: the tab's, unless another run
    /// came meanwhile or the merge isn't parked any more (brought back, which reads its run
    /// itself, or dropped).
    fn kept_run_read(&mut self, run: Option<Run>) {
        let Some(run) = run else {
            return;
        };
        let parked = self.resolver.parked.iter().any(|p| p.id == run.id);
        if self.resolver.run.is_none() && parked {
            self.resolver.run = Some(run);
            self.resolver.folded = true;
        }
    }

    /// Unfolds the agent's card (a parked merge's row clicked).
    pub(in crate::repo) fn resolver_show(&mut self) {
        self.resolver.folded = false;
    }

    /// The agent of a parked merge ended: its work is kept with the merge, the scratch worktree
    /// goes, and the user is told it is there to bring back.
    pub(super) fn keep_parked_work(&mut self, ctx: &egui::Context) {
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        let Some(ws) = run.workspace.clone() else {
            run.phase = Phase::Failed;
            return;
        };
        run.phase = Phase::Checking;
        let record = RunRecord::of(run);
        let done = done_files(&run.progress);
        let (repo, id) = (self.path.clone(), run.id.clone());
        self.then(
            ctx,
            move || {
                let dir = kept_dir(&repo, &id)?;
                keep(&ws, &dir, &record, &done)
            },
            |tab, kept, _| tab.parked_work_kept(kept),
        );
    }

    fn parked_work_kept(&mut self, kept: Result<(Workspace, Vec<(String, FileResult)>), String>) {
        let name = self.name.clone();
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        let (ws, results) = match kept {
            Ok(kept) => kept,
            Err(e) => {
                run.phase = Phase::Failed;
                run.error = Some(format!("The agent's work couldn't be kept: {e}"));
                return;
            }
        };
        let_go(run.workspace.replace(Arc::new(ws)));
        if let Some(file) = run.mcp_file.take() {
            let _ = std::fs::remove_file(file);
        }
        run.picked = results
            .iter()
            .filter(|(_, r)| r.applies())
            .map(|(p, _)| p.clone())
            .collect();
        let ready = run.picked.len();
        run.results = results;
        run.phase = Phase::Ready;
        let what = run.parked.as_ref().map(Parked::what).unwrap_or_default();
        let text = match ready {
            0 => format!("{name} · The agent is done with the {what}: nothing to apply"),
            1 => format!("{name} · The agent resolved 1 file of the {what}: bring it back"),
            n => format!("{name} · The agent resolved {n} files of the {what}: bring it back"),
        };
        self.finished
            .push(crate::repo::notice::Notice::plain((text, ready == 0)));
    }

    /// Brings the parked merge `id` back into the worktree, then shows the agent's work on it.
    pub(super) fn bring_back(&mut self, id: &str, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Wait for the running operation to finish".to_owned(),
                true,
            )));
            return;
        }
        let Some(parked) = self.resolver.parked.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        // Its agent may go on working (in its own worktree): its work is then checked against
        // the merge back in the worktree, as in place. Only while the merge is being put aside,
        // or the agent's work kept with it, does it wait a moment.
        let settling =
            self.resolver.run.as_ref().is_some_and(|r| {
                r.id == id && matches!(r.phase, Phase::Preparing | Phase::Checking)
            });
        if settling {
            self.status = Some(crate::repo::notice::Notice::plain((
                "A moment: try again in a second".to_owned(),
                true,
            )));
            return;
        }
        // Switching cards is safe only after the other parked run is durably kept.
        if self.resolver.run.as_ref().is_some_and(|r| {
            r.id != id
                && (r.parked.is_none()
                    || !matches!(r.phase, Phase::Ready | Phase::Failed)
                    || !r.workspace.as_ref().is_some_and(|w| w.is_kept()))
        }) {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Finish or dismiss the current agent run first".into(),
                true,
            )));
            return;
        }
        let repo = self.path.clone();
        self.busy_op = Some(BRINGING_BACK.to_owned());
        self.then(
            ctx,
            move || park::bring_back(&repo, &parked).map(|()| parked),
            |tab, back, ctx| tab.brought_back(back, ctx),
        );
    }

    fn brought_back(&mut self, back: Result<Parked, String>, ctx: &egui::Context) {
        if self.busy_op.as_deref() == Some(BRINGING_BACK) {
            self.busy_op = None;
        }
        let parked = match back {
            Ok(p) => p,
            Err(e) => {
                self.status = Some(crate::repo::notice::Notice::plain((
                    format!("Not brought back: {e}"),
                    true,
                )));
                return;
            }
        };
        self.resolver.parked.retain(|p| p.id != parked.id);
        self.resolver.parked_changed();
        self.status = Some(crate::repo::notice::Notice::plain((
            format!(
                "The {} is back, as it was. Recovery files remain available from its graph row menu.",
                parked.what()
            ),
            false,
        )));
        // Its changes shown, as they were: conflicted and staged files.
        self.resolver.reveal = Some((super::Reveal::Changes, Instant::now()));
        self.reload(ctx);
        if self.resolver_owns(&parked.id) {
            self.show_brought_back(ctx);
            return;
        }
        let repo = self.path.clone();
        let id = parked.id.clone();
        self.then(
            ctx,
            move || kept_run(&repo, &parked),
            move |tab, run, ctx| {
                tab.select_restored_run(&id, run, ctx);
            },
        );
    }

    fn select_restored_run(&mut self, id: &str, run: Option<Run>, ctx: &egui::Context) {
        let Some(run) = run.filter(|r| r.id == id) else {
            return;
        };
        let replaceable = self.resolver.run.as_ref().is_none_or(|r| {
            r.id == id
                || (r.parked.is_some()
                    && matches!(r.phase, Phase::Ready | Phase::Failed)
                    && r.workspace.as_ref().is_some_and(|w| w.is_kept()))
        });
        if !replaceable {
            return;
        }
        // The previous run's workspace and run.json remain with its parked merge.
        // In particular, do not call let_go, which deletes the durable workspace.
        self.resolver.run = Some(run);
        self.show_brought_back(ctx);
    }

    /// The run of the merge just brought back: its card in sight, as for a run in place, and
    /// once it is done what it did, to review and apply now that the merge is here.
    fn show_brought_back(&mut self, ctx: &egui::Context) {
        let Some(run) = self.resolver.run.as_mut() else {
            return;
        };
        run.parked = None;
        self.resolver.folded = false;
        if run.phase == Phase::Ready && !run.results.is_empty() {
            self.resolver_act(super::ResolverAct::Review(0), ctx);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod selection_regression_tests;

#[cfg(test)]
mod restored_run_tests;
