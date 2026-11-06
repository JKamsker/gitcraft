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
    let one = plan.staged == 1;
    let files = if one {
        "The staged file".to_owned()
    } else {
        format!("The {} staged files", plan.staged)
    };
    let (heading, lead) = match (plan.keep_fixup, one) {
        (true, true) => (
            "Commit staged changes as a fixup",
            "is committed as a fixup of",
        ),
        (true, false) => (
            "Commit staged changes as a fixup",
            "are committed as a fixup of",
        ),
        (false, true) => ("Fold staged changes into a commit", "goes into"),
        (false, false) => ("Fold staged changes into a commit", "go into"),
    };
    title(ui, heading, &format!("{files} {lead} this commit:"), &t);
    commit_row(
        ui,
        &plan.target,
        plan.pushed.contains(&plan.target.hash),
        &t,
    );
    // Before what changes with it, so that ticking it doesn't move it from under the pointer.
    ui.add_space(6.0);
    ui.checkbox(&mut plan.keep_fixup, "Keep as fixup! commit for later")
        .on_hover_text(
            "Commit the staged changes as “fixup! <summary>” on top of the branch, and squash \
             it later (git rebase -i --autosquash)",
        );
    ui.add_space(8.0);
    if plan.keep_fixup {
        let line = format!(
            "A commit “fixup! {}” goes on top of {}; nothing is rewritten. An interactive rebase \
             with autosquash squashes it later.",
            plan.target.summary, plan.branch
        );
        ui.add(egui::Label::new(RichText::new(line).color(t.text_dim)).wrap());
    } else {
        rewritten(ui, plan, &t);
    }
    if plan.rewrites_pushed() {
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(pushed_warning(plan)).color(t.warning)).wrap());
    }
    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            RichText::new(
                "The commit keeps its message. Unstaged and untracked changes stay as they are. \
                 Ctrl+Z undoes it.",
            )
            .color(t.text_faint)
            .small(),
        )
        .wrap(),
    );
    ui.add_space(12.0);
    let button = if plan.keep_fixup {
        "Commit as fixup"
    } else {
        "Fold"
    };
    // Rewriting what a remote has takes a click, not a stray Enter.
    let enter = enter && !plan.rewrites_pushed();
    buttons(ui, button, true, enter, || {
        Action::RunFold(Box::new(plan.clone()))
    })
}

/// The commits after the target, newest first like the graph, which the fold rewrites.
fn rewritten(ui: &mut Ui, plan: &Plan, t: &Tokens) {
    let n = plan.after.len();
    let line = match n {
        0 => "It is the latest commit: no other commit is rewritten.".to_owned(),
        1 => format!("The commit after it on {} is rewritten too:", plan.branch),
        n => format!(
            "The {n} commits after it on {} are rewritten too:",
            plan.branch
        ),
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

/// Which of the rewritten commits are already pushed, and what that means.
fn pushed_warning(plan: &Plan) -> String {
    let total = plan.after.len() + 1;
    let n = plan.pushed.len();
    let what = if total == 1 {
        "This commit is already pushed".to_owned()
    } else if n == total {
        format!("All {total} commits are already pushed")
    } else {
        format!(
            "{n} of these commits {} already pushed",
            if n == 1 { "is" } else { "are" }
        )
    };
    format!("{what}: folding rewrites shared history, and the branch will need a force push.")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(after: usize, pushed: usize) -> Plan {
        let all: Vec<RebaseCommit> = (0..=after)
            .map(|i| RebaseCommit {
                hash: format!("{i:040}"),
                ..RebaseCommit::default()
            })
            .collect();
        Plan {
            target: all[0].clone(),
            after: all[1..].to_vec(),
            pushed: all.iter().take(pushed).map(|c| c.hash.clone()).collect(),
            root: false,
            head: String::new(),
            branch: "main".to_owned(),
            staged: 1,
            keep_fixup: false,
        }
    }

    #[test]
    fn the_pushed_warning_counts_right() {
        assert!(pushed_warning(&plan(0, 1)).starts_with("This commit is already pushed"));
        assert!(pushed_warning(&plan(2, 3)).starts_with("All 3 commits are already pushed"));
        assert!(pushed_warning(&plan(2, 1)).starts_with("1 of these commits is already"));
        assert!(pushed_warning(&plan(3, 2)).starts_with("2 of these commits are already"));
    }
}
