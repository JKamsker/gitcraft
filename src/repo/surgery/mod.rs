//! History surgery on the checked-out branch: splitting a commit in two or more. The new
//! commits are prepared first without touching the working tree or the index
//! ([`crate::git::tree_after`], [`crate::git::commit_tree`]); then an interactive rebase puts
//! them in place of the old one and replays the commits after it, so a conflict goes to the
//! usual conflict flow and Ctrl+Z puts the branch back as one step.

mod absorb;
#[cfg(test)]
mod absorb_tests;
#[cfg(test)]
mod move_tests;
mod moving;
mod patchset;
mod replay;
mod split;
#[cfg(test)]
mod tests;

pub(crate) use absorb::AbsorbEditor;
pub(crate) use split::SplitEditor;

use egui::Ui;

use super::operations::expect_ref_at;
use super::quick_edit::{self, Edit};
use super::rebase::{Entry, RebaseEditor, Step};
use super::{Action, Refresh, RepoTab, Undo};
use crate::git::{self, RebaseCommit};

/// A commit a rewrite made for the todo list, as the rebase picks it.
pub(crate) fn prepared(hash: String, message: &str, of: &RebaseCommit) -> Entry {
    let summary = message.lines().next().unwrap_or_default().to_owned();
    Entry {
        commit: RebaseCommit {
            hash,
            summary,
            message: message.to_owned(),
            author: of.author.clone(),
            email: of.email.clone(),
            merge: false,
        },
        step: Step::Pick,
        message: message.to_owned(),
    }
}

/// The rebase replaying `commits` (those after `onto`, oldest first, as git lists them) as
/// `plan` turns them into a todo list; every commit is made again (`--no-ff`).
pub(crate) fn rebase_plan(
    onto: &str,
    commits: Vec<RebaseCommit>,
    plan: impl FnOnce(Vec<Entry>) -> Result<Vec<Entry>, String>,
) -> Result<RebaseEditor, String> {
    if commits.iter().any(|c| c.merge) {
        return Err(
            "merges came after the commit, which this would flatten: rebase them in the terminal with --rebase-merges"
                .to_owned(),
        );
    }
    let mut editor = RebaseEditor::new(onto.to_owned(), onto.to_owned(), commits);
    editor.entries = plan(std::mem::take(&mut editor.entries))?;
    editor.no_ff = true;
    if let Some(problem) = editor.problem() {
        return Err(problem);
    }
    Ok(editor)
}

impl RepoTab {
    /// Why the commit `hash` can't be rewritten here, if it can't: HEAD detached, something in
    /// progress, the commit not on the checked-out branch, a merge, merges after it, the first
    /// commit.
    pub(super) fn surgery_refusal(&self, hash: &str) -> Option<String> {
        if self.refs.current.is_none() {
            return Some("HEAD is detached: check out a branch first".to_owned());
        }
        if let Some(state) = self.work.in_progress {
            return Some(format!("A {} is in progress", state.label().to_lowercase()));
        }
        let line = quick_edit::first_parents(&self.history, &self.history_index, &self.refs.head);
        quick_edit::refusal(Edit::Drop, &line, hash).map(str::to_owned)
    }

    /// The entries of a commit's menu that rewrite it: _Split commit…_.
    pub(super) fn surgery_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        idle: bool,
        acts: &mut Vec<Action>,
    ) {
        let refused = self.surgery_refusal(hash);
        let button = ui
            .add_enabled(
                idle && refused.is_none(),
                egui::Button::new("Split commit…"),
            )
            .on_hover_text("Divide its changes between two or more new commits");
        let button = match &refused {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        if button.clicked() {
            acts.push(Action::OpenSplit(hash.to_owned()));
        }
        let button = ui
            .add_enabled(
                idle && refused.is_none(),
                egui::Button::new("Move changes to another commit…"),
            )
            .on_hover_text(
                "Take some of its files, hunks or lines out, into another commit of the branch or the working changes",
            );
        let button = match &refused {
            Some(why) => button.on_disabled_hover_text(why),
            None => button,
        };
        if button.clicked() {
            acts.push(Action::OpenMove(hash.to_owned()));
        }
    }

    /// Reads the commits after `onto` in the background, has `plan` make the todo list of
    /// them (it may make commits too: it runs there), and runs the rebase between the steps
    /// `around` it (those that prepare and finish the rewrite). It refuses once the branch checked out is another or HEAD moved
    /// since that read; undone as one step. `started` hears whether it can start, or why not,
    /// and says whether it still should: the editor that asked for it closed meanwhile, it
    /// doesn't.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rewrite_with(
        &mut self,
        label: String,
        onto: String,
        plan: impl FnOnce(Vec<Entry>) -> Result<Vec<Entry>, String> + Send + 'static,
        around: (Vec<Vec<String>>, Vec<Vec<String>>),
        undo: Undo,
        started: impl FnOnce(&mut RepoTab, Result<(), String>) -> bool + Send + 'static,
        ctx: &egui::Context,
    ) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let commits = git::rebase_commits(&path, &onto)?;
                let editor = rebase_plan(&onto, commits, plan)?;
                Ok((editor.command(&path)?, editor.head))
            },
            move |tab, result: Result<(Vec<String>, String), String>, ctx| match result {
                Ok(_) if tab.busy_op.is_some() => {
                    let why = format!(
                        "{} is still running, try again when it's done",
                        tab.busy_op.as_deref().unwrap_or_default()
                    );
                    let _ = started(tab, Err(why));
                }
                Ok((command, head)) => {
                    let (before, after) = around;
                    let steps = [
                        vec![tab.same_branch(), expect_ref_at("HEAD", &head)],
                        before,
                        vec![command],
                        after,
                    ]
                    .concat();
                    if started(tab, Ok(())) {
                        tab.run_undoable(&label, steps, Refresh::All, undo, ctx);
                    }
                }
                Err(e) => {
                    let _ = started(tab, Err(e));
                }
            },
        );
    }
}
