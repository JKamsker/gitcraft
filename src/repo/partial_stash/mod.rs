//! Stashing some of the changes: the files picked, only what is staged, or a snapshot that
//! saves a stash and leaves every change where it is. Each goes through the undo history: undo
//! pops the stash back (or, for a snapshot, drops it), redo makes it again.

pub(super) mod palette;
mod selection;
pub(crate) use selection::*;
mod cleanup;
pub(crate) use cleanup::*;
mod execution;
pub(crate) use execution::*;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::time::Instant;

use super::diff::literal_path;
use super::messages::spawn_mutation;
use super::operations::{
    expect_ref, expect_tree, is_expectation, run_steps_counted, stash_push_args,
};
use super::{Msg, OpKind, Refresh, RepoTab, Undo, ops};
use crate::git::{self, WorkStatus};

/// What gets stashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StashMode {
    /// Every change (`git stash push`).
    Everything,
    /// These files only (`git stash push -- <paths>`).
    Files(Vec<String>),
    /// Only what is staged (`git stash push --staged`, git 2.35+).
    Staged,
    /// Every change saved as a stash, the working tree and index left as they are.
    Snapshot,
}

/// A stash asked for in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StashRequest {
    pub(crate) mode: StashMode,
    /// Optional; git names the stash after the last commit without one.
    pub(crate) message: String,
    /// Untracked files go too (for a mode that can take them; picked ones always do).
    pub(crate) untracked: bool,
}

impl StashRequest {
    /// Every change, untracked files included: what the toolbar's Stash did all along.
    #[cfg(test)]
    pub(crate) fn everything(message: &str) -> Self {
        StashRequest {
            mode: StashMode::Everything,
            message: message.to_owned(),
            untracked: true,
        }
    }

    /// The name of the operation, in the status line and the undo history.
    pub(crate) fn label(&self) -> &'static str {
        match self.mode {
            StashMode::Everything => "Stash",
            StashMode::Files(_) => "Stash files",
            StashMode::Staged => "Stash staged",
            StashMode::Snapshot => "Snapshot",
        }
    }
}

/// Whether the repository's git can stash only the staged changes, once asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum StagedSupport {
    #[default]
    Unknown,
    Checking,
    Known(Result<(), String>),
    /// git couldn't be asked (the machine wasn't reached): asked again when the dialog opens
    /// next.
    Failed(String),
}

/// A tab's part in this: the files picked to stash (Ctrl+click in the changes), and whether
/// its git can stash only what is staged.
#[derive(Default)]
pub(crate) struct StashPicks {
    pub(crate) picked: BTreeSet<String>,
    pub(crate) staged: StagedSupport,
}

impl StashPicks {
    /// Picks or unpicks `path`.
    pub(crate) fn toggle(&mut self, path: &str) {
        if !self.picked.remove(path) {
            self.picked.insert(path.to_owned());
        }
    }

    /// Forgets the picked files that have no changes any more (stashed, committed, discarded).
    pub(crate) fn keep_changed(&mut self, work: &WorkStatus) {
        if self.picked.is_empty() {
            return;
        }
        let changed: HashSet<&str> = all_files(work).map(|f| f.path.as_str()).collect();
        self.picked.retain(|p| changed.contains(p.as_str()));
    }
}

/// Keeps of `files` those that still have changes in `work`.
pub(crate) fn keep_changed_files(files: &mut Vec<String>, work: &WorkStatus) {
    files.retain(|p| all_files(work).any(|f| &f.path == p));
}

/// Whether any of `paths` is untracked (it then needs `--include-untracked`).
pub(crate) fn has_untracked(paths: &[String], work: &WorkStatus) -> bool {
    work.unstaged
        .iter()
        .any(|f| f.area == git::Area::Untracked && paths.contains(&f.path))
}

/// The `git stash push` that makes the stash `request` asks for, or why it can't be made.
pub(crate) fn push_args(request: &StashRequest, work: &WorkStatus) -> Result<Vec<String>, String> {
    let mut args = vec!["stash".to_owned(), "push".to_owned()];
    let paths = match &request.mode {
        StashMode::Snapshot => {
            return Err(
                crate::i18n::text("repo-workflows-partial_stash-push-args-0a7314b3").into(),
            );
        }
        StashMode::Everything => {
            if let Some(problem) = push_everything_problem(work, request.untracked) {
                return Err(problem);
            }
            if request.untracked {
                args = stash_push_args().map(str::to_owned).to_vec();
            }
            None
        }
        StashMode::Staged => {
            if work.staged.is_empty() {
                return Err(
                    crate::i18n::text("repo-workflows-partial_stash-push-args-e90b2782").into(),
                );
            }
            args.push("--staged".to_owned());
            None
        }
        StashMode::Files(paths) => {
            if let Some(problem) = files_problem(paths, work) {
                return Err(problem);
            }
            if has_untracked(paths, work) {
                args.push("--include-untracked".to_owned());
            }
            Some(paths)
        }
    };
    let message = request.message.trim();
    if !message.is_empty() {
        args.extend(["-m".to_owned(), message.to_owned()]);
    }
    if let Some(paths) = paths {
        args.push("--".to_owned());
        let unique: BTreeSet<&String> = paths.iter().collect();
        args.extend(unique.into_iter().map(|p| literal_path(p)));
    }
    Ok(args)
}

impl RepoTab {
    /// Makes the stash asked for in the dialog (or the palette), in the background.
    pub(super) fn stash(&mut self, request: StashRequest, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let label = request.label();
        let head = self.refs.head.clone();
        if request.mode == StashMode::Snapshot {
            let StashRequest {
                message, untracked, ..
            } = request;
            return self.stash_job(label, ctx, move |path| {
                run_snapshot(path, &message, untracked, head)
            });
        }
        match push_args(&request, &self.work) {
            Ok(args) => {
                let staged = request.mode == StashMode::Staged;
                self.stash_job(label, ctx, move |path| run_push(path, args, head, staged));
            }
            Err(e) => {
                self.status = Some(crate::repo::Notice::plain((
                    format!("{label} failed: {e}"),
                    true,
                )))
            }
        }
    }

    /// Runs `job` in the background as the tab's operation `label`, then takes in how it ended
    /// like any other operation (status line, undo, reload).
    fn stash_job(
        &mut self,
        label: &'static str,
        ctx: &egui::Context,
        job: impl FnOnce(&Path) -> (Result<String, String>, OpKind) + Send + 'static,
    ) {
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        self.status = None;
        // Nothing here signs in: an operation run again after signing in must not be this one.
        self.last_op = None;
        let path = self.path.clone();
        let failed = move |e| done(label, Err(e), OpKind::plain());
        spawn_mutation(&self.tx, ctx, failed, move || {
            let (result, kind) = job(&path);
            done(label, result, kind)
        });
    }

    /// Starts asking the repository's git whether it can stash only the staged changes, unless
    /// it was asked already.
    pub(super) fn check_staged_stash(&mut self, ctx: &egui::Context) {
        if self.stash_picks.staged != StagedSupport::Unknown {
            return;
        }
        self.stash_picks.staged = StagedSupport::Checking;
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::staged_stash_support(&path),
            |tab, found, _| {
                tab.stash_picks.staged = match found {
                    Ok(known) => StagedSupport::Known(known),
                    Err(e) => StagedSupport::Failed(e),
                }
            },
        );
    }
}

fn done(label: &str, result: Result<String, String>, kind: OpKind) -> Msg {
    Msg::OpDone {
        label: label.to_owned(),
        result,
        refresh: Refresh::All,
        kind,
        quiet: false,
        stopped: 0,
    }
}

#[cfg(test)]
mod tests;
