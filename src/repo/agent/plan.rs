//! What an agent's request becomes: the same steps, guards and undo the window's own buttons
//! use for that operation, worked out from the repository as it is now. Run on a background
//! thread (it reads the repository with git), never on the window's.
//!
//! Everything the request names is checked again here, whatever `gitcraft mcp` checked: the
//! window trusts nothing that came over the channel but the token.

mod approval;
pub(crate) use approval::*;
mod index;
use index::*;
mod branches;
use branches::*;
mod history;
use history::*;
mod push;
use push::*;

use std::path::Path;

use super::super::dialogs::branch::create_branch_undo;
use super::super::operations::{expect_ref, expect_ref_at, expect_symref, stash_push_args};
use super::super::quick_edit::{self, Edit};
use super::super::sidebar::branch_delete::redo_delete;
use super::super::{Refresh, Undo, ops, short};
use crate::git;
use crate::mcp::args;
use crate::mcp::wire::{ResetMode, WriteOp};

/// What the window runs for a request, and what it asks the user first, if anything.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    /// The operation's name in the status line and the undo list.
    pub label: String,
    pub ops: Vec<Vec<String>>,
    pub refresh: Refresh,
    pub undo: Undo,
    /// Shown on the Approve / Deny card, for an operation that can't be undone or loses work.
    pub ask: Option<Ask>,
}

/// The repository as a plan needs it: HEAD's commit (empty before the first commit) and the
/// branch checked out (`None` when detached).
struct Here {
    head: String,
    branch: Option<String>,
}

impl Here {
    fn read(repo: &Path) -> Self {
        let branch = git::run_ok_codes(
            Some(repo),
            &["symbolic-ref", "-q", "--short", "HEAD"],
            &[0, 1],
        )
        .map(|s| s.trim().to_owned())
        .ok()
        .filter(|s| !s.is_empty());
        Self {
            head: git::head(repo),
            branch,
        }
    }

    /// What is checked out, as undo goes back to it: the branch, or the commit when detached.
    fn checked_out(&self) -> String {
        self.branch.clone().unwrap_or_else(|| self.head.clone())
    }

    /// The branch checked out, or why an operation on it can't be done.
    fn branch(&self) -> Result<&str, String> {
        self.branch
            .as_deref()
            .ok_or_else(|| crate::i18n::text("repo-workflows-agent-branch-7776bacf").to_owned())
    }

    /// The guard that the same branch (or a detached HEAD) is still checked out.
    fn same_checkout(&self) -> Vec<String> {
        let full = self
            .branch
            .as_ref()
            .map(|b| format!("refs/heads/{b}"))
            .unwrap_or_default();
        expect_symref("HEAD", &full)
    }
}

/// The plan for `op` in `repo`, or why it can't be done.
pub(crate) fn plan(repo: &Path, op: &WriteOp) -> Result<Plan, String> {
    let here = Here::read(repo);
    match op {
        WriteOp::Stage { paths } => stage(repo, &here, paths),
        WriteOp::Unstage { paths } => unstage(repo, &here, paths),
        WriteOp::Commit { message, amend } => commit(repo, &here, message, *amend),
        WriteOp::CreateBranch {
            name,
            start,
            checkout,
        } => create_branch(repo, &here, name, start.as_deref(), *checkout),
        WriteOp::RenameBranch { from, to } => rename_branch(repo, from, to),
        WriteOp::Checkout { branch } => checkout(repo, &here, branch),
        WriteOp::Stash { message } => stash(repo, message),
        WriteOp::CherryPick { commit } => replay(repo, commit, "Cherry-pick", &["cherry-pick"]),
        WriteOp::Revert { commit } => replay(repo, commit, "Revert", &["revert", "--no-edit"]),
        WriteOp::Reset { commit, mode } => reset(repo, &here, commit, *mode),
        WriteOp::MoveCommit { commit, up } => {
            let edit = if *up { Edit::MoveUp } else { Edit::MoveDown };
            rewrite(repo, &here, commit, edit)
        }
        WriteOp::DropCommit { commit } => rewrite(repo, &here, commit, Edit::Drop),
        WriteOp::DeleteBranch { name, force } => delete_branch(repo, &here, name, *force),
        WriteOp::Push { force } => push(repo, &here, *force),
        // Taken by the run it is for before it reaches a tab (see `app::agent_card`).
        WriteOp::Report { .. } => Err("a report is not an operation".to_owned()),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fluent_ask_receipt_tests;
