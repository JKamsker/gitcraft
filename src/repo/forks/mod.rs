//! Pull requests from forks: where each comes from and what its author allows (badges in the
//! list), checking a fork's branch out so that pushing goes back to the contributor, rebasing it
//! onto its base and pushing it to them, approving workflow runs waiting for a maintainer (on
//! GitHub), and removing the remotes added for forks once nothing uses them.

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
    /// A rebase that stopped on conflicts: once it is continued, its push is offered.
    after_conflict: Option<plan::Rebase>,
    /// The pull request whose runs "Approve and run" was chosen for once: chosen again, they
    /// are approved.
    approve_armed: Option<u64>,
    /// Runs are being approved.
    approving: bool,
}

/// What the views ask of the forks.
pub(crate) enum ForkAct {
    /// Check out the branch of the pull request to push to it.
    Checkout(u64),
    /// Rebase the checked-out branch of the pull request onto its base and push it.
    Rebase(u64),
    /// Approve the workflow runs waiting for the pull request's latest commit.
    Approve(u64),
    /// Ask before removing the fork remotes nothing uses.
    Tidy,
}

impl Forks {
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
        self.then(
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
        if self.busy_op.is_some() {
            return;
        }
        match act {
            ForkAct::Checkout(number) => self.checkout_fork(number, ctx),
            ForkAct::Rebase(number) => self.plan_fork_rebase(number, ctx),
            ForkAct::Approve(number) => self.approve_runs(number, ctx),
            ForkAct::Tidy => self.plan_tidy(ctx),
        }
    }

    fn checkout_fork(&mut self, number: u64, ctx: &Context) {
        let Some(info) = self.forks.of(number).cloned() else {
            self.status = Some((
                crate::i18n::message(
                    "repo-workflows-forks-checkout-fork-5ecb3bac",
                    &[("number", (number).clone().into())],
                ),
                true,
            ));
            return;
        };
        let locals: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let previous = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| self.refs.head.clone());
        match plan::checkout(&info, &self.remotes, &self.forks.setup, &locals, &previous) {
            Ok(plan) => {
                let label = crate::i18n::message(
                    "repo-workflows-forks-checkout-fork-abaaf4c3",
                    &[
                        ("number", (number).clone().into()),
                        ("info-owner-1", (info.owner).clone().into()),
                    ],
                );
                self.run_undoable(
                    &label,
                    plan.steps.clone(),
                    Refresh::All,
                    Undo::Custom {
                        undo: plan.undo,
                        redo: plan.steps,
                    },
                    ctx,
                );
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    fn plan_fork_rebase(&mut self, number: u64, ctx: &Context) {
        let Some(info) = self.forks.of(number).cloned() else {
            return;
        };
        let Some(upstream) = self.default_remote() else {
            self.status = Some((
                crate::i18n::text("repo-workflows-forks-plan-fork-rebase-1abac341").to_owned(),
                true,
            ));
            return;
        };
        let repo = self.path.clone();
        let setup = self.forks.setup.clone();
        let creds = self.credentials.clone();
        self.status = Some((
            crate::i18n::message(
                "repo-workflows-forks-plan-fork-rebase-ac71056e",
                &[("number", (number).clone().into())],
            ),
            false,
        ));
        self.then(
            ctx,
            move || {
                plan::plan_rebase(&repo, &info, &upstream, &setup, creds.as_ref())
                    .map(|p| (info, p))
            },
            |tab, plan, _| match plan {
                Ok((info, plan)) => {
                    tab.status = None;
                    let label = format!("Rebase #{} onto {} and push", info.number, info.base);
                    let body = rebase_preview(&info, &plan);
                    tab.dialog = Some(
                        Dialog::confirm(
                            &label,
                            body,
                            crate::i18n::text("repo-workflows-forks-plan-fork-rebase-a3647cfa"),
                            plan.steps(),
                            None,
                        )
                        .undo(Undo::Custom {
                            undo: plan.undo_steps(),
                            redo: plan.redo_steps(),
                        }),
                    );
                    tab.forks.running = Some((label, plan));
                }
                Err(e) => tab.status = Some((e, true)),
            },
        );
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
                        tab.status = Some((
                            crate::i18n::message(
                                "repo-workflows-forks-plan-tidy-adf73b14",
                                &[("e", (e).clone().into())],
                            ),
                            true,
                        ));
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
                    tab.status = Some((
                        crate::i18n::message(
                            "repo-workflows-forks-plan-tidy-da56e1d6",
                            &[("kept", (kept).clone().into())],
                        ),
                        false,
                    ));
                    return;
                }
                tab.dialog = Some(
                    Dialog::confirm(
                        "Remove unused fork remotes",
                        crate::i18n::message(
                            "repo-workflows-forks-plan-tidy-b732e7ba",
                            &[
                                ("unused-join-1", (unused.join(", ")).clone().into()),
                                ("kept", (kept).clone().into()),
                            ],
                        ),
                        crate::i18n::text("repo-workflows-forks-plan-tidy-c3812fc4"),
                        steps.clone(),
                        None,
                    )
                    .undo(Undo::Custom { undo, redo: steps }),
                );
            },
        );
    }

    /// Once a rebase that stopped on conflicts is continued, asks to push it to the fork; once
    /// it is aborted, forgets it.
    fn offer_push_after_conflict(&mut self) {
        let Some(plan) = &self.forks.after_conflict else {
            return;
        };
        if self.work.in_progress.is_some() || self.loading || self.dialog.is_some() {
            return;
        }
        let plan = plan.clone();
        self.forks.after_conflict = None;
        let here = self.refs.current.as_deref() == Some(plan.local.as_str());
        if !here || self.refs.head == plan.head {
            return;
        }
        let label = format!("Push the rebased {} to {}", plan.local, plan.fork);
        self.dialog = Some(
            Dialog::confirm(
                &label,
                crate::i18n::message(
                    "repo-workflows-forks-offer-push-after-conflict-dcc709d6",
                    &[
                        ("plan-local-1", (plan.local).clone().into()),
                        ("plan-fork-2", (plan.fork).clone().into()),
                        ("plan-branch-3", (plan.branch).clone().into()),
                    ],
                ),
                crate::i18n::text("repo-workflows-forks-offer-push-after-conflict-731ce7ed"),
                plan.push_now(),
                None,
            )
            .undo(Undo::Custom {
                undo: plan.fork_undo_steps(),
                redo: plan.push_now(),
            }),
        );
    }

    fn approve_runs(&mut self, number: u64, ctx: &Context) {
        if self.forks.approving {
            return;
        }
        // Chosen once: say what it means. Chosen again: go ahead.
        if self.forks.approve_armed != Some(number) {
            self.forks.approve_armed = Some(number);
            self.status = Some((
                crate::i18n::message(
                    "repo-workflows-forks-approve-runs-a77eca22",
                    &[("number", (number).clone().into())],
                ),
                false,
            ));
            return;
        }
        self.forks.approve_armed = None;
        let Some(info) = self.forks.of(number) else {
            return;
        };
        let runs = self.forks.waiting(&info.sha).to_vec();
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        if runs.is_empty() {
            return;
        }
        let repo = self.path.clone();
        self.forks.approving = true;
        self.status = Some((
            crate::i18n::message(
                "repo-workflows-forks-approve-runs-2f659b28",
                &[("number", (number).clone().into())],
            ),
            false,
        ));
        self.then(
            ctx,
            move || {
                runs.iter()
                    .map(|run| hosting.forge.provider().approve_run(&repo, &hosting, *run))
                    .find(Result::is_err)
                    .unwrap_or(Ok(()))
            },
            move |tab, result, ctx| {
                tab.forks.approving = false;
                tab.status = Some(match result {
                    Ok(()) => (
                        crate::i18n::message(
                            "repo-workflows-forks-approve-runs-a151ad19",
                            &[("number", (number).clone().into())],
                        ),
                        false,
                    ),
                    Err(e) => (
                        crate::i18n::message(
                            "repo-workflows-forks-approve-runs-282082c7",
                            &[
                                ("number", (number).clone().into()),
                                ("e", (e).clone().into()),
                            ],
                        ),
                        true,
                    ),
                });
                // Read again: they are running now.
                tab.forks.read_for = None;
                tab.refresh_ci(ctx);
            },
        );
    }

    /// Takes in how an operation ended: a rebase whose push failed after rewriting the branch
    /// is made undoable here, and signing in pushes it.
    pub(super) fn forks_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        ctx: &Context,
    ) {
        let Some((asked, plan)) = self.forks.running.take_if(|(l, _)| l == label) else {
            return;
        };
        let Err(error) = result else { return };
        let auth = crate::git::is_auth_error(error);
        let conflicts = error.contains("CONFLICT") || error.contains("could not apply");
        let repo = self.path.clone();
        let local = format!("refs/heads/{}", plan.local);
        self.then(
            ctx,
            move || crate::git::resolve(&repo, &local).unwrap_or_default(),
            move |tab, now, ctx| {
                // Stopped by its conflicts: once they are resolved and the rebase continued,
                // the push is offered.
                if conflicts {
                    tab.forks.after_conflict = Some(plan);
                    tab.status = Some((
                        crate::i18n::text("repo-workflows-forks-forks-op-done-9c338e90").to_owned(),
                        true,
                    ));
                    return;
                }
                // Stopped before the rebase: nothing rewritten to undo.
                if now.is_empty() || now == plan.head {
                    return;
                }
                let (undo, redo) = plan.local_undo(&now);
                tab.push_undo(
                    UndoEntry {
                        id: 0,
                        agent: None,
                        label: format!("{asked} (here only)"),
                        undo,
                        redo,
                        heads: None,
                    },
                    ctx,
                );

                tab.status = Some((
                    crate::i18n::message(
                        "repo-workflows-forks-forks-op-done-8287ad6b",
                        &[
                            ("plan-local-1", (plan.local).clone().into()),
                            ("plan-fork-2", (plan.fork).clone().into()),
                        ],
                    ),
                    true,
                ));
                if auth
                    && let Some(op) = &mut tab.last_op
                    && op.label == asked
                {
                    // Undoable on the fork too once pushed; the rewrite here has its own entry.
                    *op = PendingOp {
                        label: format!("Push {} to {}", plan.local, plan.fork),
                        ops: plan.push_now(),
                        refresh: Refresh::All,
                        kind: OpKind::Normal(
                            Undo::Custom {
                                undo: plan.fork_undo_steps(),
                                redo: plan.push_now(),
                            },
                            String::new(),
                        ),
                        quiet: false,
                    };
                }
            },
        );
    }
}

/// What the rebase dialog says it will do.
fn rebase_preview(info: &ForkInfo, plan: &plan::Rebase) -> String {
    format!(
        "Rebases {} ({}'s branch {} of #{}) onto {}, which has {} commit{} it hasn't, then \
         force-pushes it to {} (their fork) with a lease: refused if {} pushed there since it \
         was fetched a moment ago.\n\nIf the rebase stops on conflicts, resolve them and \
         continue it: gitcraft then offers to push it. Abort it, and nothing is pushed.\n\n\
         Ctrl+Z puts the branch back as it was, on the fork and here.",
        plan.local,
        info.owner,
        info.branch,
        info.number,
        plan.onto
            .strip_prefix("refs/remotes/")
            .unwrap_or(&plan.onto),
        plan.behind,
        if plan.behind == 1 { "" } else { "s" },
        plan.fork,
        info.owner,
    )
}
