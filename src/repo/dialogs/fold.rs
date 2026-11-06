//! The dialog that asks before folding the staged changes into a commit: the commit, the
//! commits after it that get rewritten (and which of them are pushed), and the choice of a
//! `fixup!` commit kept for later instead.

use egui::{RichText, Ui};

use crate::git::RebaseCommit;
use crate::repo::fold::Plan;
use crate::repo::{Action, short};
use crate::theme::{self, Tokens};

use super::{Outcome, buttons, title};

/// How many of the rewritten commits are listed; the rest are counted.
const LISTED: usize = 8;

pub(super) fn fold(ui: &mut Ui, enter: bool, plan: &mut Plan) -> Outcome {
    let t = Tokens::get(ui.ctx());
    let files = match plan.staged {
        1 => "The staged file goes into this commit:".to_owned(),
        n => format!("The {n} staged files go into this commit:"),
    };
    title(ui, "Fold staged changes into a commit", &files, &t);
    commit_row(
        ui,
        &plan.target,
        plan.pushed.contains(&plan.target.hash),
        &t,
    );
    ui.add_space(10.0);
    if plan.keep_fixup {
        let line = format!(
            "A commit “fixup! {}” goes on top of {}; nothing is rewritten. An interactive rebase \
             with autosquash melds it in later.",
            plan.target.summary, plan.branch
        );
        ui.add(egui::Label::new(RichText::new(line).color(t.text_dim)).wrap());
    } else {
        rewritten(ui, plan, &t);
    }
    if plan.rewrites_pushed() {
        ui.add_space(6.0);
        let n = plan.pushed.len();
        let line = format!(
            "{n} of these commit{} {} already pushed: the branch will need a force push.",
            if n == 1 { "" } else { "s" },
            if n == 1 { "is" } else { "are" },
        );
        ui.add(egui::Label::new(RichText::new(line).color(t.warning)).wrap());
    }
    ui.add_space(8.0);
    ui.checkbox(&mut plan.keep_fixup, "Keep as fixup! commit for later")
        .on_hover_text(
            "Commit the staged changes as “fixup! <summary>” on top of the branch, and squash \
             it later (git rebase -i --autosquash)",
        );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Unstaged and untracked changes stay as they are. Ctrl+Z undoes it.")
            .color(t.text_faint)
            .small(),
    );
    ui.add_space(12.0);
    let button = if plan.keep_fixup {
        "Commit fixup!"
    } else {
        "Fold"
    };
    buttons(ui, button, true, enter, || {
        Action::RunFold(Box::new(plan.clone()))
    })
}

/// The commits after the target, newest first like the graph, which the fold rewrites.
fn rewritten(ui: &mut Ui, plan: &Plan, t: &Tokens) {
    let n = plan.after.len();
    let line = match n {
        0 => "It is the latest commit: only it is rewritten.".to_owned(),
        1 => "The commit after it is rewritten too:".to_owned(),
        n => format!("The {n} commits after it are rewritten too:"),
    };
    ui.label(RichText::new(line).color(t.text_dim));
    for commit in plan.after.iter().rev().take(LISTED) {
        commit_row(ui, commit, plan.pushed.contains(&commit.hash), t);
    }
    if n > LISTED {
        ui.label(
            RichText::new(format!("… and {} older ones", n - LISTED))
                .color(t.text_faint)
                .small(),
        );
    }
}

/// A commit's short hash and summary, marked when a remote already has it.
fn commit_row(ui: &mut Ui, commit: &RebaseCommit, pushed: bool, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(short(&commit.hash))
                .font(theme::mono(11.5))
                .color(t.text_faint),
        );
        if pushed {
            ui.label(RichText::new("pushed").color(t.warning).small())
                .on_hover_text("A remote branch already has this commit");
        }
        ui.add(egui::Label::new(RichText::new(&commit.summary).color(t.text)).truncate());
    });
}
