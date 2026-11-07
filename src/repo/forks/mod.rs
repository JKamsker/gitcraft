//! Pull requests from forks: where each comes from and what its author allows (badges in the
//! list), checking a fork's branch out so that pushing goes back to the contributor, rebasing it
//! onto its base and pushing it to them, approving workflow runs waiting for a maintainer (on
//! GitHub), and removing the remotes added for forks once nothing uses them.

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

    fn checkout_fork(&mut self, number: u64, ctx: &Context) {
        let Some(info) = self.forks.of(number).cloned() else {
            self.status = Some((format!("Where #{number} comes from isn't known yet"), true));
            return;
        };
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let Some(primary) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .cloned()
        else {
            return;
        };
        let locals: Vec<String> = self.refs.local.iter().map(|b| b.name.clone()).collect();
        let previous = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| self.refs.head.clone());
        match plan::checkout(
            &info,
            &self.remotes,
            &self.forks.setup,
            &locals,
            &previous,
            self.hosting.as_ref(),
        ) {
            Ok(plan) => {
                let label = format!("Check out #{number} to push to {}", info.owner);
                let url = self
                    .remotes
                    .iter()
                    .find(|r| r.name == plan.remote)
                    .map_or_else(|| plan.url.clone(), |r| r.url.clone());
                let fork_hosting = crate::forge::Hosting {
                    path: info.repo_path.clone(),
                    ..hosting.clone()
                };
                self.replace_fork_scope(
                    guard::Scope::new(self.forge_generation, hosting, primary).with_fork(
                        plan.remote.clone(),
                        url,
                        fork_hosting,
                        plan.new_remote,
                    ),
                );
                self.forks.scope_label = Some(label.clone());
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
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        let Some(upstream) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .filter(|r| hosting.matches_git_remote(&r.url))
            .cloned()
        else {
            self.status = Some((
                "This repository has no remote to rebase onto".to_owned(),
                true,
            ));
            return;
        };
        let repo = self.path.clone();
        let setup = self.forks.setup.clone();
        let scope = guard::Scope::new(self.forge_generation, hosting.clone(), upstream.clone());
        self.replace_fork_scope(scope.clone());
        let creds = self.credentials.clone();
        #[cfg(test)]
        let recorded = tests::take_plan_recording(&upstream.name);
        let work_scope = scope.clone();
        self.status = Some((format!("Fetching #{number}'s branch and its base…"), false));
        self.forge_then(
            ctx,
            move || {
                #[cfg(test)]
                if let Some(recorded) = recorded {
                    return recorded().map(|p| (info, p));
                }
                plan::plan_rebase(
                    &repo,
                    &info,
                    &upstream,
                    &setup,
                    creds.as_ref(),
                    Some(&hosting),
                    Some(&work_scope),
                )
                .map(|p| (info, p))
            },
            move |tab, plan, _| {
                if scope.check().is_err() {
                    return;
                }
                match plan {
                    Ok((info, plan)) => {
                        tab.status = None;
                        let label = format!("Rebase #{} onto {} and push", info.number, info.base);
                        let body = rebase_preview(&info, &plan);
                        tab.dialog = Some(
                            Dialog::confirm(&label, body, "Rebase and push", plan.steps(), None)
                                .instead_ops(super::Action::Fork(ForkAct::ConfirmRebase(scope.id)))
                                .undo(Undo::Custom {
                                    undo: plan.undo_steps(),
                                    redo: plan.redo_steps(),
                                }),
                        );
                        tab.forks.scope_label = Some(label.clone());
                        tab.forks.running = Some((label, plan));
                    }
                    Err(e) => {
                        tab.forks.scope = None;
                        tab.status = Some((e, true));
                    }
                }
            },
        );
    }

    fn replace_fork_scope(&mut self, scope: guard::Scope) {
        if let Some(old) = &self.forks.scope {
            old.invalidate();
        }
        if matches!(&self.dialog, Some(Dialog::Confirm { preview, .. })
            if preview.instead.as_ref().is_some_and(|i| matches!(i.action,
                super::Action::Fork(ForkAct::ConfirmRebase(_) | ForkAct::ConfirmPush(_)))))
        {
            self.dialog = None;
        }
        self.forks.running = None;
        self.forks.after_conflict = None;
        self.forks.scope_label = None;
        self.forks.scope = Some(scope);
    }

    fn confirm_fork_rebase(&mut self, id: u64, push_only: bool, ctx: &Context) {
        self.sync_forge_selection(ctx);
        let valid = self.forks.scope.as_ref().is_some_and(|s| {
            s.id == id
                && s.generation == self.forge_generation
                && s.check().is_ok()
                && self
                    .hosting
                    .as_ref()
                    .is_some_and(|h| h.same_repository(&s.hosting))
                && self.primary_forge_remote() == Some(s.primary.name.as_str())
                && self.remotes.iter().any(|r| r == &s.primary)
        });
        if !valid {
            self.status = Some((
                "The primary forge changed; plan the fork action again".into(),
                true,
            ));
            return;
        }
        let Some((label, plan)) = self.forks.running.clone() else {
            return;
        };
        let (steps, undo) = if push_only {
            (plan.push_now(), plan.fork_undo_steps())
        } else {
            (plan.steps(), plan.undo_steps())
        };
        let redo = if push_only {
            plan.push_now()
        } else {
            plan.redo_steps()
        };
        self.run_undoable(
            &label,
            steps,
            Refresh::All,
            Undo::Custom { undo, redo },
            ctx,
        );
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
        let running = active.then(|| self.forks.running.take()).flatten();
        let scope = active.then(|| self.forks.scope.take()).flatten();
        let scope_label = active.then(|| self.forks.scope_label.take()).flatten();
        self.forks = Forks {
            running,
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
                        tab.status = Some((format!("Couldn't read the remotes: {e}"), true));
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
                    tab.status = Some((format!("No fork remote can go{kept}"), false));
                    return;
                }
                tab.dialog = Some(
                    Dialog::confirm(
                        "Remove unused fork remotes",
                        format!(
                            "Removes the remotes gitcraft added for forks that no branch pulls \
                             from or pushes to any more: {}, with their remote-tracking \
                             branches. Ctrl+Z adds them back (fetch them for their \
                             branches).{kept}",
                            unused.join(", ")
                        ),
                        "Remove",
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
        let Some(scope) = self
            .forks
            .scope
            .as_ref()
            .filter(|s| s.check().is_ok() && s.generation == self.forge_generation)
        else {
            self.forks.after_conflict = None;
            return;
        };
        let id = scope.id;
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
                format!(
                    "The rebase is done. Force-pushes {} to {}'s {} with a lease: refused if \
                     they pushed there since it was fetched. Ctrl+Z puts their branch back as \
                     it was (and yours with the next Ctrl+Z).",
                    plan.local, plan.fork, plan.branch
                ),
                "Push",
                plan.push_now(),
                None,
            )
            .instead_ops(super::Action::Fork(ForkAct::ConfirmPush(id)))
            .undo(Undo::Custom {
                undo: plan.fork_undo_steps(),
                redo: plan.push_now(),
            }),
        );
        self.forks.scope_label = Some(label.clone());
        self.forks.running = Some((label, plan));
    }

    fn approve_runs(&mut self, number: u64, ctx: &Context) {
        if self.forks.approving {
            return;
        }
        // Chosen once: say what it means. Chosen again: go ahead.
        if self.forks.approve_armed != Some(number) {
            self.forks.approve_armed = Some(number);
            self.status = Some((
                format!(
                    "Approving runs the contributor's code on your CI: look through #{number}'s \
                     changes first, then choose Approve and run again"
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
        let Some(primary) = self
            .primary_forge_remote()
            .and_then(|name| self.remotes.iter().find(|r| r.name == name))
            .cloned()
        else {
            return;
        };
        let scope = guard::Scope::new(self.forge_generation, hosting.clone(), primary);
        self.replace_fork_scope(scope.clone());
        let label = format!("Approve #{number}'s workflow runs");
        self.forks.scope_label = Some(label.clone());
        self.busy_op = Some(label);
        self.op_started = Some(Instant::now());
        self.forks.approving = true;
        self.status = Some((format!("Approving #{number}'s workflow runs…"), false));
        let sender = self.forge_operation_sender();
        #[cfg(test)]
        let mut recorded = tests::take_approval_recording();
        super::messages::spawn_mutation(
            &sender,
            ctx,
            move |e| approval_result(number, Err(e)),
            move || {
                let result: Result<(), String> = (|| {
                    for run in runs {
                        scope.check_remote(&repo)?;
                        #[cfg(test)]
                        if let Some(record) = &mut recorded {
                            record(run)?;
                            continue;
                        }
                        hosting.forge.provider().approve_run(&repo, &hosting, run)?;
                    }
                    Ok(())
                })();
                approval_result(number, result)
            },
        );
    }

    pub(super) fn stale_fork_operation_finished(&mut self) {
        if self
            .forks
            .scope
            .as_ref()
            .is_some_and(|s| s.check().is_err())
        {
            self.forks.running = None;
            self.forks.scope = None;
            self.forks.scope_label = None;
            self.forks.approving = false;
        }
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
            if result.is_ok() && self.forks.scope_label.as_deref() == Some(label) {
                self.forks.scope = None;
                self.forks.scope_label = None;
            }
            return;
        };
        let Err(error) = result else { return };
        let scope = self.forks.scope.clone();
        let generation = self.forge_generation;
        if scope.as_ref().is_some_and(|s| s.check().is_err()) {
            self.last_op = None;
            if matches!(self.dialog, Some(Dialog::Credentials { .. })) {
                self.dialog = None;
            }
        }
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
                let current = scope.as_ref().is_some_and(|s| {
                    s.check().is_ok()
                        && s.generation == tab.forge_generation
                        && generation == tab.forge_generation
                });
                if conflicts && current {
                    tab.forks.after_conflict = Some(plan);
                    tab.status = Some((
                        "The rebase stopped on conflicts: resolve them and continue it, then \
                         gitcraft offers to push it (or abort it)"
                            .to_owned(),
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
                    format!(
                        "{} is rebased here, but not pushed to {}: Ctrl+Z puts it back as it was",
                        plan.local, plan.fork
                    ),
                    true,
                ));
                if !current && tab.last_op.as_ref().is_some_and(|o| o.label == asked) {
                    tab.last_op = None;
                }
                if auth
                    && current
                    && let Some(op) = &mut tab.last_op
                    && op.label == asked
                {
                    // Undoable on the fork too once pushed; the rewrite here has its own entry.
                    let label = format!("Push {} to {}", plan.local, plan.fork);
                    tab.forks.scope_label = Some(label.clone());
                    *op = PendingOp {
                        label,
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

fn approval_result(number: u64, result: Result<(), String>) -> super::Msg {
    super::Msg::Then(Box::new(move |tab, ctx| {
        tab.busy_op = None;
        tab.op_started = None;
        tab.forks.scope = None;
        tab.forks.scope_label = None;
        tab.forks.approving = false;
        tab.status = Some(match result {
            Ok(()) => (format!("#{number}'s workflow runs are approved"), false),
            Err(e) => (
                format!("Couldn't approve #{number}'s workflow runs: {e}"),
                true,
            ),
        });
        tab.forks.read_for = None;
        tab.refresh_ci(ctx);
    }))
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
