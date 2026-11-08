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
        crate::i18n::text("repo-core-1301").to_owned()
    } else {
        crate::i18n::message("repo-core-1302", &[("staged", format!("{}", plan.staged).into())])
    };
    let (heading, lead) = match (plan.keep_fixup, one) {
        (true, true) => (
            crate::i18n::text("repo-core-1303"),
            crate::i18n::text("repo-core-1304"),
        ),
        (true, false) => (
            crate::i18n::text("repo-core-1303"),
            crate::i18n::text("repo-core-1305"),
        ),
        (false, true) => (crate::i18n::text("repo-core-1306"), crate::i18n::text("repo-core-1307")),
        (false, false) => (crate::i18n::text("repo-core-1306"), crate::i18n::text("repo-core-1308")),
    };
    title(ui, heading, &crate::i18n::message("repo-core-1309", &[("files", format!("{}", files).into()), ("lead", format!("{}", lead).into())]), &t);
    commit_row(
        ui,
        &plan.target,
        plan.pushed.contains(&plan.target.hash),
        &t,
    );
    // Before what changes with it, so that ticking it doesn't move it from under the pointer.
    ui.add_space(6.0);
    ui.checkbox(&mut plan.keep_fixup, crate::i18n::text("repo-core-1310"))
        .on_hover_text(
            crate::i18n::text("repo-core-1311"),
        );
    ui.add_space(8.0);
    if plan.keep_fixup {
        let line = crate::i18n::message("repo-core-1312", &[("summary", format!("{}", plan.target.summary).into()), ("branch", format!("{}", plan.branch).into())]);
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
                crate::i18n::text("repo-core-1313"),
            )
            .color(t.text_faint)
            .small(),
        )
        .wrap(),
    );
    ui.add_space(12.0);
    let button = if plan.keep_fixup {
        crate::i18n::text("repo-core-1314")
    } else {
        crate::i18n::text("repo-core-1315")
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
        0 => crate::i18n::text("repo-core-1316").to_owned(),
        1 => crate::i18n::message("repo-core-1317", &[("branch", format!("{}", plan.branch).into())]),
        n => crate::i18n::message("repo-core-1318", &[("n", (n).into()), ("branch", format!("{}", plan.branch).into())]),
    };
    ui.label(RichText::new(line).color(t.text_dim));
    for commit in plan.after.iter().rev().take(LISTED) {
        commit_row(ui, commit, plan.pushed.contains(&commit.hash), t);
    }
    if n > LISTED {
        ui.label(
            RichText::new(crate::i18n::message("repo-core-1319", &[("LISTED", format!("{}", n - LISTED).into())]))
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
        crate::i18n::text("repo-core-1320").to_owned()
    } else if n == total {
        crate::i18n::message("repo-core-1321", &[("total", (total).into())])
    } else {
        crate::i18n::message("repo-core-1322", &[("n", (n).into()), (crate::i18n::text("repo-core-1324"), format!("{}", if n == 1 { crate::i18n::text("repo-core-1323") } else { crate::i18n::text("repo-core-1324") }).into())])
    };
    crate::i18n::message("repo-core-1325", &[("what", format!("{}", what).into())])
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
            ui.label(RichText::new(crate::i18n::text("repo-core-1326")).color(t.warning).small())
                .on_hover_text(crate::i18n::text("repo-core-1327"));
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
