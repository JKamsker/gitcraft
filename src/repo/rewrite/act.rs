//! What rewriting the checked-out branch is asked: rewording, the interactive rebase, folding
//! staged changes in, dropping or moving commits, and patches. The root
//! [`Action`](crate::repo::Action) routes these here in one arm.

use crate::repo::{RepoTab, drag_drop, fold, quick_edit};

pub(in crate::repo) enum RewriteAct {
    /// Gives a commit of the checked-out branch a new message: amending the latest, rebasing
    /// for an older one.
    Reword { hash: String, message: String },
    /// Open the editor of a commit's message in the detail pane.
    EditMessage(String),
    /// Save a commit as a patch file, asking where.
    SavePatch { hash: String, summary: String },
    /// Apply a patch file, asking which.
    ApplyPatch,
    /// Open the menu of what a drop offers.
    DropMenu(drag_drop::DropMenu),
    /// Move or squash a commit of the checked-out branch onto another of it.
    CommitDrop(drag_drop::CommitDrop),
    /// Drop or move a commit of the checked-out branch.
    QuickEdit {
        edit: quick_edit::Edit,
        hash: String,
        branch_guard: Vec<String>,
    },
    /// Open the interactive rebase editor for the commits after `onto`.
    InteractiveRebase { onto: String, label: String },
    /// Ask before folding the staged changes into the commit `hash` (once what that rewrites
    /// is read).
    FoldInto(String),
    /// Fold as the dialog confirmed it.
    RunFold(Box<fold::Plan>),
    /// Abort the rebase of a fold stopped on a conflict, asked first once it is checked to be
    /// the fold's.
    AbortFold,
    RunRebase,
    ConfirmedRebase(u64),
    AskRebase { onto: String, label: String },
    CloseRebase,
}

impl RepoTab {
    pub(in crate::repo) fn rewrite_act(&mut self, act: RewriteAct, ctx: &egui::Context) {
        match act {
            RewriteAct::InteractiveRebase { onto, label } => {
                self.interactive_rebase(onto, label, ctx)
            }
            RewriteAct::Reword { hash, message } => self.reword_commit(hash, message, ctx),
            RewriteAct::EditMessage(hash) => self.edit_message(hash, ctx),
            RewriteAct::SavePatch { hash, summary } => self.save_patch(hash, &summary, ctx),
            RewriteAct::ApplyPatch => self.apply_patch(ctx),
            RewriteAct::QuickEdit {
                edit,
                hash,
                branch_guard,
            } => self.quick_edit(edit, hash, branch_guard, ctx),
            RewriteAct::DropMenu(menu) => self.drop_menu = Some(menu),
            RewriteAct::CommitDrop(drop) => self.commit_drop(drop, ctx),
            RewriteAct::RunRebase => self.ask_run_rebase(ctx),
            RewriteAct::ConfirmedRebase(id) => {
                self.confirmed_rebase(id, ctx);
            }
            RewriteAct::AskRebase { onto, label } => self.ask_plain_rebase(onto, label, ctx),
            RewriteAct::FoldInto(hash) => self.fold_into(hash, ctx),
            RewriteAct::RunFold(plan) => self.run_fold(*plan, ctx),
            RewriteAct::AbortFold => self.abort_fold(ctx),
            RewriteAct::CloseRebase => self.rebase = None,
        }
    }
}
