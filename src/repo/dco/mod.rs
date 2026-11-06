//! DCO sign-off: the commit box's _Sign off_ toggle (on by itself where the repository asks for
//! one), the commits flagged that have no `Signed-off-by` from their author, and _Sign off my
//! commits on this branch_ on a pull request, which rewrites them and pushes them back.

mod signoff;
#[cfg(test)]
mod tests;
mod view;

use egui::Context;

use super::{Action, Dialog, Refresh, RepoTab, Undo};
use crate::pull_request::CheckState;

pub(crate) use signoff::{Plan, Signed, is_dco_check, signed_off_by};
pub(crate) use view::{pr_section, pr_shows, unsigned};

/// What the tab knows of the repository's sign-off rules.
#[derive(Default)]
pub(crate) struct Dco {
    rules: signoff::Rules,
    /// The read (`history_gen`) the rules were read for.
    read_for: u64,
    /// The rules are being read.
    reading: bool,
    /// A pull request of the repository has a failing DCO check: its number.
    failing_check: Option<u64>,
    /// The sign-offs of the commits of the pull request shown, if read.
    pr: Option<PrSignoffs>,
}

/// The commits of a pull request, with whether each is signed off.
pub(crate) struct PrSignoffs {
    pub number: u64,
    /// The pull request's latest commit, which the list is of.
    pub head: String,
    /// Where its changes start (the merge base).
    pub base: String,
    pub commits: Result<Vec<Signed>, String>,
}

/// What the views ask of the sign-off.
pub(crate) enum DcoAct {
    /// The commit box's toggle was set.
    SetSignOff(bool),
    /// Plan signing off the commits of the checked-out branch since `base`, and ask.
    PlanFix { base: String },
}

/// Why the commit box signs off by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    File(String),
    FailingCheck(u64),
}

impl Reason {
    pub(crate) fn text(&self) -> String {
        match self {
            Reason::File(file) => format!("this repository has a {file} file"),
            Reason::FailingCheck(n) => format!("the DCO check of #{n} fails"),
        }
    }
}

impl Dco {
    /// Why the repository asks for a sign-off, if it does.
    pub(crate) fn reason(&self) -> Option<Reason> {
        self.rules
            .file
            .clone()
            .map(Reason::File)
            .or(self.failing_check.map(Reason::FailingCheck))
    }

    /// Whether new commits are signed off: as chosen in the commit box, else as the repository
    /// asks.
    pub(crate) fn signs_off(&self) -> bool {
        self.rules.choice.unwrap_or(self.reason().is_some())
    }

    /// Whether the repository asks for sign-offs, so commits without one are flagged.
    pub(crate) fn required(&self) -> bool {
        self.reason().is_some() || self.rules.choice == Some(true)
    }

    /// The sign-offs of pull request `number` at `head`, if read.
    pub(crate) fn for_pr(&self, number: u64, head: &str) -> Option<&PrSignoffs> {
        self.pr
            .as_ref()
            .filter(|p| p.number == number && p.head == head)
    }
}

impl RepoTab {
    /// Each frame: reads the rules again after a reload, notices a failing DCO check on the pull
    /// request shown and reads the sign-offs of its commits once they are here.
    pub(super) fn dco_frame(&mut self, ctx: &Context) {
        if self.dco.read_for != self.history_gen && !self.dco.reading && self.history_gen > 0 {
            self.dco.reading = true;
            let read_for = self.history_gen;
            let repo = self.path.clone();
            self.then(
                ctx,
                move || signoff::read_rules(&repo),
                move |tab, rules, _| {
                    tab.dco.reading = false;
                    tab.dco.read_for = read_for;
                    tab.dco.rules = rules;
                },
            );
        }
        let Some(view) = &self.pr_view else { return };
        let Some(Ok(details)) = &view.details else {
            return;
        };
        if details
            .checks
            .iter()
            .any(|c| c.state == CheckState::Failed && is_dco_check(&c.name))
        {
            self.dco.failing_check = Some(details.number);
        } else if self.dco.failing_check == Some(details.number) {
            self.dco.failing_check = None;
        }
        let Some(Ok(files)) = &view.files else { return };
        if files.to != details.head_oid
            || self
                .dco
                .pr
                .as_ref()
                .is_some_and(|p| p.number == details.number && p.head == files.to)
        {
            return;
        }
        let (number, head, base) = (details.number, files.to.clone(), files.from.clone());
        let repo = self.path.clone();
        let range = format!("{base}..{head}");
        // Marked as read now, so it is asked once; filled in when it comes.
        self.dco.pr = Some(PrSignoffs {
            number,
            head: head.clone(),
            base: base.clone(),
            commits: Ok(Vec::new()),
        });
        self.then(
            ctx,
            move || signoff::commits(&repo, &range),
            move |tab, commits, _| {
                if let Some(pr) = &mut tab.dco.pr
                    && pr.number == number
                    && pr.head == head
                {
                    pr.commits = commits;
                }
            },
        );
    }

    pub(super) fn dco_act(&mut self, act: DcoAct, ctx: &Context) {
        match act {
            DcoAct::SetSignOff(on) => {
                self.dco.rules.choice = Some(on);
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || {
                        let args = signoff::choice_command(on);
                        let args: Vec<&str> = args.iter().map(String::as_str).collect();
                        crate::git::run(Some(&repo), &args)
                    },
                    |tab, result, _| {
                        if let Err(e) = result {
                            tab.status =
                                Some((format!("Couldn't keep the sign-off choice: {e}"), true));
                        }
                    },
                );
            }
            DcoAct::PlanFix { base } => {
                if self.busy_op.is_some() {
                    return;
                }
                let repo = self.path.clone();
                let email = self.identity.email.clone();
                let who = format!("{} <{}>", self.identity.name, self.identity.email);
                self.then(
                    ctx,
                    move || signoff::plan(&repo, &base),
                    move |tab, plan, _| match plan {
                        Ok(plan) => tab.dialog = Some(fix_dialog(&plan, &who, &email)),
                        Err(e) => tab.status = Some((e, true)),
                    },
                );
            }
        }
    }

    /// Whether the commit is made with `--signoff`.
    pub(super) fn signs_off(&self) -> bool {
        self.dco.signs_off()
    }
}

/// The dialog that shows what signing off the branch does, and does it.
fn fix_dialog(plan: &Plan, who: &str, email: &str) -> Dialog {
    Dialog::confirm(
        &format!("Sign off the commits of {}", plan.branch),
        fix_preview(plan, who, email),
        "Sign off and push",
        plan.steps(),
        None,
    )
    .undo(Undo::Custom {
        undo: plan.undo_steps(),
        redo: plan.redo_steps(),
    })
    .refresh(Refresh::All)
}

/// What [`fix_dialog`] says it will do.
pub(crate) fn fix_preview(plan: &Plan, who: &str, email: &str) -> String {
    let unsigned = plan.unsigned();
    let n = plan.commits.len();
    let mut text = format!(
        "Rewrites the {n} commit{} of {} since {} (rebase --signoff), adding \
         \"Signed-off-by: {who}\" to each that doesn't have it. {} of them had no sign-off from \
         their author:\n",
        if n == 1 { "" } else { "s" },
        plan.branch,
        super::short(&plan.base),
        unsigned.len(),
    );
    for c in unsigned.iter().take(8) {
        text.push_str(&format!("  • {} {}\n", super::short(&c.hash), c.summary));
    }
    if unsigned.len() > 8 {
        text.push_str(&format!("  • and {} more\n", unsigned.len() - 8));
    }
    let others = plan.by_others(email);
    if !others.is_empty() {
        let mut names: Vec<&str> = others.iter().map(|c| c.author.as_str()).collect();
        names.dedup();
        text.push_str(&format!(
            "\n{} of them {} by {}: your sign-off is added to them too, but the DCO check \
             wants their author's own.\n",
            others.len(),
            if others.len() == 1 { "is" } else { "are" },
            names.join(", "),
        ));
    }
    match &plan.push {
        Some(push) => {
            let there = push
                .remote_ref
                .strip_prefix("refs/heads/")
                .unwrap_or(&push.remote_ref);
            text.push_str(&format!(
                "\nThen force-pushes it to {}/{there} with a lease: refused if someone pushed \
                 there since it was last fetched.\n",
                push.remote
            ));
        }
        None => text.push_str("\nThe branch has no remote branch to push to: push it yourself.\n"),
    }
    text.push_str("\nCtrl+Z puts the commits as they were back, here and on the remote.");
    text
}

/// The action that asks to sign off the checked-out branch's commits since `base`.
pub(crate) fn plan_fix(base: &str) -> Action {
    Action::Dco(DcoAct::PlanFix {
        base: base.to_owned(),
    })
}
