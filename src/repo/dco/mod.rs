//! DCO sign-off: the commit box's _Sign off_ toggle (on by itself where the repository asks for
//! one), the commits flagged that the DCO check would refuse, and _Sign off the branch's
//! commits_ on a pull request, which rewrites them and pushes them back.

mod signoff;
#[cfg(test)]
mod tests;
mod view;

use egui::Context;

use super::state::PendingOp;
use super::{Action, Dialog, OpKind, Refresh, RepoTab, Undo, UndoEntry};
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
    /// Counts the choices written from the commit box: a read of the rules started before the
    /// latest one has the choice from before it.
    writes: u64,
    /// The pull request shown has a failing DCO check: its number.
    failing_check: Option<u64>,
    /// The sign-offs of the commits of the pull request shown, if asked for.
    pr: Option<PrSignoffs>,
    /// The sign-off asked for in the dialog, or running: what a failed push leaves to undo.
    running: Option<Plan>,
}

/// The commits of a pull request, with whether each is signed off.
pub(crate) struct PrSignoffs {
    pub number: u64,
    /// The pull request's latest commit, which the list is of.
    pub head: String,
    /// Where its changes start (the merge base).
    pub base: String,
    /// `None` until read.
    pub commits: Option<Result<Vec<Signed>, String>>,
}

/// What the views ask of the sign-off.
pub(crate) enum DcoAct {
    /// The commit box's toggle was set.
    SetSignOff(bool),
    /// Plan signing off the commits of the checked-out branch since `base`, which must be at
    /// `head` (the pull request's latest commit), and ask.
    PlanFix { base: String, head: String },
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

    /// Whether the repository asks for sign-offs, so commits without one are flagged. Signing
    /// off by choice flags nothing.
    pub(crate) fn required(&self) -> bool {
        self.reason().is_some()
    }

    /// The sign-offs of pull request `number` at `head`, if asked for.
    pub(crate) fn for_pr(&self, number: u64, head: &str) -> Option<&PrSignoffs> {
        self.pr
            .as_ref()
            .filter(|p| p.number == number && p.head == head)
    }
}

impl RepoTab {
    /// Each frame: reads the rules again after a reload, notices a failing DCO check on the pull
    /// request shown and, where sign-offs are asked for, reads those of its commits once they
    /// are here.
    pub(super) fn dco_frame(&mut self, ctx: &Context) {
        if self.dco.read_for != self.history_gen && !self.dco.reading && self.history_gen > 0 {
            self.read_dco_rules(ctx);
        }
        let shown = self
            .pr_view
            .as_ref()
            .and_then(|view| view.details.as_ref()?.as_ref().ok().map(|d| (view, d)));
        // Only while it is shown: a pull request looked at once doesn't sign off every commit
        // made after.
        self.dco.failing_check = shown.and_then(|(_, details)| {
            details
                .checks
                .iter()
                .any(|c| c.state == CheckState::Failed && is_dco_check(&c.name))
                .then_some(details.number)
        });
        let Some((view, details)) = shown else { return };
        let Some(Ok(files)) = &view.files else { return };
        if files.to != details.head_oid
            || !self.dco.required()
            || self.dco.for_pr(details.number, &files.to).is_some()
        {
            return;
        }
        let (number, head, base) = (details.number, files.to.clone(), files.from.clone());
        let repo = self.path.clone();
        let range = format!("{base}..{head}");
        // Marked as asked now, so it is asked once; filled in when it comes.
        self.dco.pr = Some(PrSignoffs {
            number,
            head: head.clone(),
            base,
            commits: None,
        });
        self.then(
            ctx,
            move || signoff::commits(&repo, &range),
            move |tab, commits, _| {
                if let Some(pr) = &mut tab.dco.pr
                    && pr.number == number
                    && pr.head == head
                {
                    pr.commits = Some(commits);
                }
            },
        );
    }

    /// Reads the rules in the background. A panic there (a bug) reads as no rules, rather than
    /// leaving them never read again.
    fn read_dco_rules(&mut self, ctx: &Context) {
        self.dco.reading = true;
        let read_for = self.history_gen;
        let writes = self.dco.writes;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || std::panic::catch_unwind(|| signoff::read_rules(&repo)).unwrap_or_default(),
            move |tab, rules, _| {
                tab.dco.reading = false;
                tab.dco.read_for = read_for;
                // A choice made since the read started is newer than the one it read.
                let choice = if tab.dco.writes == writes {
                    rules.choice
                } else {
                    tab.dco.rules.choice
                };
                tab.dco.rules = signoff::Rules { choice, ..rules };
            },
        );
    }

    pub(super) fn dco_act(&mut self, act: DcoAct, ctx: &Context) {
        match act {
            DcoAct::SetSignOff(on) => {
                // Set to what the repository asks anyway: back to following it.
                let automatic = self.dco.reason().is_some();
                let choice = (on != automatic).then_some(on);
                self.dco.rules.choice = choice;
                self.dco.writes += 1;
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || {
                        let args = signoff::choice_command(choice);
                        let args: Vec<&str> = args.iter().map(String::as_str).collect();
                        // Exit 5: nothing to take out.
                        crate::git::run_ok_codes(Some(&repo), &args, &[0, 5])
                    },
                    |tab, result, _| {
                        if let Err(e) = result {
                            tab.status = Some((
                                format!(
                                    "Couldn't save Sign off for this repository \
                                     (git config {}): {e}",
                                    signoff::SIGN_OFF_KEY
                                ),
                                true,
                            ));
                        }
                    },
                );
            }
            DcoAct::PlanFix { base, head } => {
                if self.busy_op.is_some() {
                    return;
                }
                let repo = self.path.clone();
                let email = self.identity.email.clone();
                let who = format!("{} <{}>", self.identity.name, self.identity.email);
                self.then(
                    ctx,
                    move || signoff::plan(&repo, &base, Some(&head)),
                    move |tab, plan, _| match plan {
                        Ok(plan) => {
                            tab.dialog = Some(fix_dialog(&plan, &who, &email));
                            tab.dco.running = Some(plan);
                        }
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

    /// Takes in how an operation ended. When the sign-off of a branch failed after rewriting
    /// it (its push was refused, or needs signing in), the rewrite is made undoable here, and
    /// signing in pushes it.
    pub(super) fn dco_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &Context,
    ) {
        let Some(plan) = self.dco.running.take_if(|plan| fix_label(plan) == label) else {
            return;
        };
        let Err(error) = result else { return };
        let auth = crate::git::is_auth_error(error);
        let repo = self.path.clone();
        let local = format!("refs/heads/{}", plan.branch);
        self.then(
            ctx,
            move || crate::git::resolve(&repo, &local).unwrap_or_default(),
            move |tab, now, ctx| {
                if now.is_empty() || now == plan.head {
                    return;
                }
                let (undo, redo) = plan.local_undo(&now);
                tab.push_undo(
                    UndoEntry {
                        id: 0,
                        agent: None,
                        label: format!("Sign off the commits of {} (here only)", plan.branch),
                        undo,
                        redo,
                        heads: None,
                    },
                    ctx,
                );

                let push = plan.push.as_ref().map(signoff::PushTarget::label);
                tab.status = Some((
                    format!(
                        "{}'s commits are signed off here, but not pushed{}: Ctrl+Z puts them \
                         back as they were",
                        plan.branch,
                        push.map(|p| format!(" to {p}")).unwrap_or_default()
                    ),
                    true,
                ));
                // Signing in pushes what is signed off now, not the whole rewrite again.
                if auth
                    && let Some(op) = &mut tab.last_op
                    && op.label == fix_label(&plan)
                {
                    *op = PendingOp {
                        label: format!("Push the signed-off commits of {}", plan.branch),
                        ops: plan.push_steps(),
                        refresh: Refresh::All,
                        kind: OpKind::plain(),
                        quiet: false,
                    };
                }
            },
        );
    }
}

/// What the sign-off of `plan` is called: the operation's label, and the dialog's title.
fn fix_label(plan: &Plan) -> String {
    format!("Sign off the commits of {}", plan.branch)
}

/// The dialog that shows what signing off the branch does, and does it.
fn fix_dialog(plan: &Plan, who: &str, email: &str) -> Dialog {
    let button = if plan.push.is_some() {
        "Sign off and push"
    } else {
        "Sign off"
    };
    Dialog::confirm(
        &fix_label(plan),
        fix_preview(plan, who, email),
        button,
        plan.steps(),
        None,
    )
    .undo(Undo::Custom {
        undo: plan.undo_steps(),
        redo: plan.redo_steps(),
    })
    .refresh(Refresh::All)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What [`fix_dialog`] says it will do.
pub(crate) fn fix_preview(plan: &Plan, who: &str, email: &str) -> String {
    let unsigned = plan.unsigned();
    let mut text = format!(
        "Rewrites the {} of {} since {} (rebase --signoff), adding \"Signed-off-by: {who}\" \
         to each that doesn't have it. {} the DCO check refuses now:\n",
        plural(plan.commits.len(), "commit", "commits"),
        plan.branch,
        super::short(&plan.base),
        if unsigned.len() == 1 {
            "1 of them is one".to_owned()
        } else {
            format!("{} of them are ones", unsigned.len())
        },
    );
    for c in unsigned.iter().take(8) {
        text.push_str(&format!("  • {} {}\n", super::short(&c.hash), c.summary));
    }
    if unsigned.len() > 8 {
        text.push_str(&format!("  • and {} more\n", unsigned.len() - 8));
    }
    let others = plan.by_others(email);
    if !others.is_empty() {
        let mut names: Vec<&str> = Vec::new();
        for c in &others {
            if !names.contains(&c.author.as_str()) {
                names.push(&c.author);
            }
        }
        text.push_str(&format!(
            "\n{} by {}: you become their committer, and your sign-off certifies them. Only \
             go ahead if you may.\n",
            if others.len() == 1 {
                "1 of them is".to_owned()
            } else {
                format!("{} of them are", others.len())
            },
            names.join(", "),
        ));
    }
    match &plan.push {
        Some(push) => {
            let guard = if push.seen.is_some() {
                "refused if someone pushed there since it was last fetched"
            } else {
                "refused if that branch exists there already"
            };
            text.push_str(&format!(
                "\nThen force-pushes it to {} with a lease: {guard}.\n\nCtrl+Z puts the \
                 commits as they were back, on the remote and here.",
                push.label()
            ));
        }
        None => text.push_str(
            "\nThe branch has no remote branch: nothing is pushed. Pushing it later replaces \
             the old commits there (a force push).\n\nCtrl+Z puts the commits as they were back.",
        ),
    }
    text
}

/// The action that asks to sign off the checked-out branch's commits since `base`, while it is
/// at `head`.
pub(crate) fn plan_fix(base: &str, head: &str) -> Action {
    Action::Dco(DcoAct::PlanFix {
        base: base.to_owned(),
        head: head.to_owned(),
    })
}
