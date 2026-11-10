//! Pull requests from forks: where each comes from and what its author allows (badges in the
//! list), checking a fork's branch out so that pushing goes back to the contributor, rebasing it
//! onto its base and pushing it to them, approving workflow runs waiting for a maintainer (on
//! GitHub), and removing the remotes added for forks once nothing uses them.

mod completion;
mod planning;

pub(super) mod guard;
pub(crate) mod plan;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::time::Instant;

use egui::Context;

use super::state::PendingOp;
use super::{Dialog, OpKind, Refresh, RepoTab, Undo, UndoEntry};
use crate::forge::ForkInfo;
use crate::git;

#[cfg(test)]
pub(in crate::repo) fn before_step(op: &[String]) {
    tests::before_step(op);
}

pub(crate) use guard::Scope as ExecutionScope;
pub(crate) use plan::Setup;
pub(crate) use view::paint_badges;

/// What the tab knows of where the pull requests come from.
#[derive(Default)]
pub(crate) struct Forks {
    /// By pull request number.
    info: HashMap<u64, ForkInfo>,
    /// The workflow runs waiting for approval, by commit.
    waiting: HashMap<String, Vec<u64>>,
    /// The list of pull requests (as read at that moment) the info was read for.
    read_for: Option<Instant>,
    error: Option<String>,
    /// How the branches and fork remotes are set up, and the read it is from.
    setup: Setup,
    setup_for: u64,
    reading_setup: bool,
    /// The rebase asked for in the dialog, or running: what a failed push leaves to undo.
    running: Option<(String, plan::Rebase)>,
    scope: Option<guard::Scope>,
    scope_label: Option<String>,
    /// Only the local worker completion may schedule its separately reviewed push.
    staged_rebase: Option<String>,
    /// A rebase that stopped on conflicts: once it is continued, its push is offered.
    after_conflict: Option<plan::Rebase>,
    /// The completed conflict continuation captured when offering its confirmation.
    push_tip: Option<String>,
    /// The pull request whose runs "Approve and run" was chosen for once: chosen again, they
    /// are approved.
    approve_armed: Option<u64>,
    /// Runs are being approved.
    approving: bool,
}

/// What the views ask of the forks.
pub(crate) enum ForkAct {
    /// Check out the branch of the pull request to push to it.
    Checkout(u64, u64),
    /// Rebase the checked-out branch of the pull request onto its base and push it.
    Rebase(u64, u64),
    /// A confirmation carries no executable commands; recheck its captured lifetime.
    ConfirmRebase(u64),
    ConfirmPush(u64),
    /// Approve the workflow runs waiting for the pull request's latest commit.
    Approve(u64, u64),
    /// Ask before removing the fork remotes nothing uses.
    Tidy,
}

impl Forks {
    /// Consume a worker completion once; repeated delivery cannot record local
    /// undo twice or schedule the same push preview again.
    fn take_rebase_completion(&mut self, label: &str) -> Option<(bool, String, plan::Rebase)> {
        let (asked, plan) = self.running.take_if(|(l, _)| l == label)?;
        let staged = self.staged_rebase.take_if(|l| l == label).is_some();
        Some((staged, asked, plan))
    }

    /// What the forge said of pull request `number`.
    pub(crate) fn of(&self, number: u64) -> Option<&ForkInfo> {
        self.info.get(&number)
    }

    /// The runs waiting for approval of the commit `sha`.
    pub(crate) fn waiting(&self, sha: &str) -> &[u64] {
        self.waiting.get(sha).map_or(&[], Vec::as_slice)
    }

    /// How the branches and fork remotes are set up, as last read.
    pub(crate) fn setup(&self) -> &Setup {
        &self.setup
    }

    /// The fork remotes nothing uses any more.
    pub(crate) fn unused(&self) -> Vec<String> {
        self.setup.unused_forks()
    }

    /// Why the forge couldn't say where the pull requests come from, if it couldn't.
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl RepoTab {
    /// Each frame: asks the forge where the pull requests come from once their list is read, and
    /// reads how the branches are set up after each reload.
    pub(super) fn forks_frame(&mut self, ctx: &Context) {
        if self.forks.running.as_ref().is_some_and(|(label, _)| {
            self.busy_op.as_ref() != Some(label)
                && self.dialog.is_none()
                && self.last_op.as_ref().is_none_or(|o| &o.label != label)
        }) {
            self.forks.running = None;
            if let Some(scope) = self.forks.scope.take() {
                scope.invalidate();
            }
            self.forks.scope_label = None;
        }
        if self.history_gen > 0
            && self.forks.setup_for != self.history_gen
            && !self.forks.reading_setup
        {
            self.forks.reading_setup = true;
            let read_for = self.history_gen;
            let repo = self.path.clone();
            self.then(
                ctx,
                move || plan::read_setup(&repo),
                move |tab, setup, _| {
                    tab.forks.reading_setup = false;
                    tab.forks.setup_for = read_for;
                    if let Ok(setup) = setup {
                        tab.forks.setup = setup;
                    }
                },
            );
        }
        self.offer_push_after_conflict();
        let Some(loaded) = self.prs_loaded else {
            return;
        };
        if self.forks.read_for == Some(loaded) || self.prs.is_empty() {
            return;
        }
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        self.forks.read_for = Some(loaded);
        let repo = self.path.clone();
        self.forge_then(
            ctx,
            move || {
                let provider = hosting.forge.provider();
                let forks = provider.forks(&repo, &hosting);
                // Runs wait for approval on pull requests from forks only.
                let any_fork = forks.as_ref().is_ok_and(|l| l.iter().any(|i| i.cross));
                let waiting = if any_fork {
                    provider.waiting_runs(&repo, &hosting).unwrap_or_default()
                } else {
                    Vec::new()
                };
                (hosting, forks, waiting)
            },
            move |tab, (hosting, forks, waiting), _| {
                // Asked of the repository shown, for the list shown.
                if tab.hosting.as_ref() != Some(&hosting) || tab.forks.read_for != Some(loaded) {
                    return;
                }
                match forks {
                    Ok(list) => {
                        tab.forks.info = list.into_iter().map(|i| (i.number, i)).collect();
                        tab.forks.error = None;
                    }
                    // What was said before may be of pull requests since closed: nothing is
                    // offered from it.
                    Err(e) => {
                        tab.forks.info.clear();
                        tab.forks.error = Some(e);
                    }
                }
                tab.forks.waiting.clear();
                for (sha, run) in waiting {
                    tab.forks.waiting.entry(sha).or_default().push(run);
                }
            },
        );
    }

    pub(super) fn fork_act(&mut self, act: ForkAct, ctx: &Context) {
        self.sync_forge_selection(ctx);
        if self.busy_op.is_some() {
            return;
        }
        match act {
            ForkAct::Checkout(number, generation) if generation == self.forge_generation => {
                self.checkout_fork(number, ctx)
            }
            ForkAct::Rebase(number, generation) if generation == self.forge_generation => {
                self.plan_fork_rebase(number, ctx)
            }
            ForkAct::ConfirmRebase(id) => self.confirm_fork_rebase(id, false, ctx),
            ForkAct::ConfirmPush(id) => self.confirm_fork_rebase(id, true, ctx),
            ForkAct::Approve(number, generation) if generation == self.forge_generation => {
                self.approve_runs(number, ctx)
            }
            ForkAct::Checkout(..) | ForkAct::Rebase(..) | ForkAct::Approve(..) => {}
            ForkAct::Tidy => self.plan_tidy(ctx),
        }
    }

    /// Invalidate pending confirmations and the next step of already running operations.
    /// Generic OpDone is still delivered so completed local changes can be undone.
    pub(super) fn clear_forge_forks(&mut self) {
        if let Some(scope) = &self.forks.scope {
            scope.invalidate();
        }
        if matches!(&self.dialog, Some(Dialog::Confirm { preview, .. })
            if preview.instead.as_ref().is_some_and(|i| matches!(i.action,
                super::Action::Fork(ForkAct::ConfirmRebase(_) | ForkAct::ConfirmPush(_)))))
        {
            self.dialog = None;
        }
        if self
            .last_op
            .as_ref()
            .is_some_and(|o| self.forks.scope_label.as_deref() == Some(&o.label))
        {
            self.last_op = None;
        }
        let active = self
            .forks
            .scope_label
            .as_ref()
            .is_some_and(|l| self.busy_op.as_ref() == Some(l));
        let staged_rebase = active.then(|| self.forks.staged_rebase.take()).flatten();
        let running = active.then(|| self.forks.running.take()).flatten();
        let scope = active.then(|| self.forks.scope.take()).flatten();
        let scope_label = active.then(|| self.forks.scope_label.take()).flatten();
        self.forks = Forks {
            running,
            staged_rebase,
            scope,
            scope_label,
            ..Forks::default()
        };
    }

    pub(super) fn fork_execution_scope(&self, label: &str) -> Option<guard::Scope> {
        (self.forks.scope_label.as_deref() == Some(label))
            .then(|| self.forks.scope.clone())
            .flatten()
    }

    /// Reads again which fork remotes nothing uses (a branch made in a terminal since counts),
    /// leaves out those whose branches hold commits no local branch has, and asks.
    fn plan_tidy(&mut self, ctx: &Context) {
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let setup = plan::read_setup(&repo)?;
                let remotes = git::remotes(&repo)?;
                let mut keep = Vec::new();
                let mut unused = Vec::new();
                for name in setup.unused_forks() {
                    let only_there = git::run(
                        Some(&repo),
                        &[
                            "rev-list",
                            "--count",
                            &format!("--remotes={name}/*"),
                            "--not",
                            "--branches",
                        ],
                    )
                    .map(|n| n.trim() != "0")
                    .unwrap_or(true);
                    if only_there {
                        keep.push(name);
                    } else {
                        unused.push(name);
                    }
                }
                Ok::<_, String>((plan::tidy(&remotes, &unused), unused, keep))
            },
            |tab, found, _| {
                let ((steps, undo), unused, keep) = match found {
                    Ok(found) => found,
                    Err(e) => {
                        tab.status = Some(crate::repo::Notice::plain((
                            format!("Couldn't read the remotes: {e}"),
                            true,
                        )));
                        return;
                    }
                };
                let kept = if keep.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n\nKept: {}, whose branches have commits no branch here has.",
                        keep.join(", ")
                    )
                };
                if steps.is_empty() {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("No fork remote can go{kept}"),
                        false,
                    )));
                    return;
                }
                tab.dialog = Some(
                    Dialog::confirm(
                        "Remove unused fork remotes",
                        tidy_body(&unused, &keep),
                        crate::i18n::text("repo-workflows-forks-plan-tidy-c3812fc4"),
                        steps.clone(),
                        None,
                    )
                    .undo(Undo::Custom { undo, redo: steps }),
                );
            },
        );
    }
}

fn approval_result(number: u64, result: Result<(), String>) -> super::Msg {
    super::Msg::Then(Box::new(move |tab, ctx| {
        tab.busy_op = None;
        tab.op_started = None;
        tab.forks.scope = None;
        tab.forks.scope_label = None;
        tab.forks.approving = false;
        tab.status = Some(crate::repo::Notice::plain(match result {
            Ok(()) => (format!("#{number}'s workflow runs are approved"), false),
            Err(e) => (
                format!("Couldn't approve #{number}'s workflow runs: {e}"),
                true,
            ),
        }));
        tab.forks.read_for = None;
        tab.refresh_ci(ctx);
    }))
}

/// What the rebase dialog says it will do.
fn rebase_preview(info: &ForkInfo, plan: &plan::Rebase) -> String {
    crate::i18n::message(
        "repo-workflows-fork-rebase-preview",
        &[
            ("local", (plan.local.as_str()).into()),
            ("owner", (info.owner.as_str()).into()),
            ("branch", (info.branch.as_str()).into()),
            ("number", (info.number.to_string()).into()),
            (
                "onto",
                (plan
                    .onto
                    .strip_prefix("refs/remotes/")
                    .unwrap_or(&plan.onto))
                .into(),
            ),
            ("behind", (plan.behind).into()),
            ("fork", (plan.fork.as_str()).into()),
        ],
    )
}

fn tidy_body(unused: &[String], keep: &[String]) -> String {
    crate::i18n::message(
        "repo-workflows-forks-tidy-complete-body",
        &[
            ("unused", unused.join(", ").into()),
            ("kept", (if keep.is_empty() { "no" } else { "yes" }).into()),
            ("names", keep.join(", ").into()),
        ],
    )
}
#[cfg(test)]
mod fluent_tidy_tests;

/// Dispatch identity; only the canonical repository paint map localizes its presentation.
fn checkout_operation_label(number: u64, owner: &str) -> String {
    format!("Check out #{number} to push to {owner}")
}
#[cfg(test)]
mod fluent_checkout_identity_tests;

#[cfg(test)]
mod staged_recording_tests;
